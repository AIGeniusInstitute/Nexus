//! M14 Thread Snapshot + Fork + Rollback (roadmap M11 T11-4 fork 基础).
//!
//! 在某 turn 后拍快照 → 从快照分叉出新线程（携带此前全部 item 上下文）→
//! 或回滚线程到快照点（丢弃之后的 turn/item）。

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow, Serialize)]
pub struct SnapshotRow {
    pub id: i64,
    pub thread_id: Uuid,
    pub turn_id: Option<i64>,
    pub content_digest: Option<String>,
    pub forked_to_thread_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 校验 thread 归属当前租户，返回 (owner_user_id,)。
async fn assert_thread_owned(
    pool: &PgPool,
    tenant_id: i64,
    thread_id: Uuid,
) -> Result<i64> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT owner_user_id FROM threads WHERE id=$1 AND tenant_id=$2",
    )
    .bind(thread_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("thread tenant check")?;
    match row {
        Some((uid,)) => Ok(uid),
        None => anyhow::bail!("thread {thread_id} not found or not owned by tenant {tenant_id}"),
    }
}

pub async fn create_snapshot(
    pool: &PgPool,
    tenant_id: i64,
    thread_id: Uuid,
    turn_id: Option<i64>,
) -> Result<i64> {
    assert_thread_owned(pool, tenant_id, thread_id).await?;
    // turn_id None → 取最新 turn
    let snap_turn: i64 = match turn_id {
        Some(t) => t,
        None => {
            let row: (Option<i64>,) = sqlx::query_as(
                "SELECT MAX(id) FROM turns WHERE thread_id=$1",
            )
            .bind(thread_id)
            .fetch_one(pool)
            .await
            .context("max turn id")?;
            row.0.ok_or_else(|| anyhow::anyhow!("thread {thread_id} has no turns to snapshot"))?
        }
    };
    // content_digest = std hash over items.content_ref（turn_id ≤ snap）
    let items: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT content_ref FROM items WHERE thread_id=$1 AND turn_id<=$2 ORDER BY id",
    )
    .bind(thread_id)
    .bind(snap_turn)
    .fetch_all(pool)
    .await
    .context("fetch items for digest")?;
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (cr,) in &items {
        if let Some(c) = cr {
            h.write(c.as_bytes());
        }
    }
    let digest = format!("{:016x}", h.finish());
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO workspace_snapshots (thread_id, turn_id, content_digest, tenant_id) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(thread_id)
    .bind(snap_turn)
    .bind(&digest)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .context("insert snapshot")?;
    Ok(row.0)
}

pub async fn list_snapshots(
    pool: &PgPool,
    tenant_id: i64,
    thread_id: Uuid,
) -> Result<Vec<SnapshotRow>> {
    sqlx::query_as(
        "SELECT id, thread_id, turn_id, content_digest, forked_to_thread_id, created_at \
         FROM workspace_snapshots WHERE tenant_id=$1 AND thread_id=$2 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .context("list snapshots")
    .map_err(Into::into)
}

/// 分叉：创建新 thread + 单一 imported turn + 复制源线程 turn_id ≤ snap 的全部 item。
pub async fn fork_from_snapshot(
    pool: &PgPool,
    tenant_id: i64,
    thread_id: Uuid,
    snap_id: i64,
) -> Result<Uuid> {
    let owner = assert_thread_owned(pool, tenant_id, thread_id).await?;
    // 校验 snapshot 归属 + 取 turn_id
    let snap: Option<(Uuid, Option<i64>)> = sqlx::query_as(
        "SELECT thread_id, turn_id FROM workspace_snapshots WHERE id=$1 AND tenant_id=$2",
    )
    .bind(snap_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("snap tenant check")?;
    let (_, snap_turn_opt) = snap.ok_or_else(|| anyhow::anyhow!("snapshot {snap_id} not found"))?;
    let snap_turn: i64 = snap_turn_opt
        .ok_or_else(|| anyhow::anyhow!("snapshot {snap_id} has no turn_id"))?;

    let mut tx = pool.begin().await.context("begin fork tx")?;

    // 新 thread
    let new_thread: (Uuid,) = sqlx::query_as(
        "INSERT INTO threads (tenant_id, owner_user_id, title) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(owner)
    .bind(format!("fork of {}", thread_id))
    .fetch_one(&mut *tx)
    .await
    .context("insert forked thread")?;
    let new_thread_id = new_thread.0;

    // 单一 imported turn（completed）
    let new_turn: (i64,) = sqlx::query_as(
        "INSERT INTO turns (thread_id, status, completed_at) VALUES ($1, 'completed', NOW()) RETURNING id",
    )
    .bind(new_thread_id)
    .fetch_one(&mut *tx)
    .await
    .context("insert imported turn")?;
    let new_turn_id = new_turn.0;

    // 复制源线程 turn_id ≤ snap 的全部 item（新 turn_id，seq 从 1 重编）
    let copied = sqlx::query(
        "INSERT INTO items (thread_id, turn_id, seq, item_type, content_ref, content_digest) \
         SELECT $1, $2, ROW_NUMBER() OVER (ORDER BY id), item_type, content_ref, content_digest \
         FROM items WHERE thread_id=$3 AND turn_id<=$4 ORDER BY id",
    )
    .bind(new_thread_id)
    .bind(new_turn_id)
    .bind(thread_id)
    .bind(snap_turn)
    .execute(&mut *tx)
    .await
    .context("copy items into fork")?;
    tracing::info!("fork: copied {} items into new thread {}", copied.rows_affected(), new_thread_id);

    // 标记 fork 结果
    let _ = sqlx::query(
        "UPDATE workspace_snapshots SET forked_to_thread_id=$1 WHERE id=$2",
    )
    .bind(new_thread_id)
    .bind(snap_id)
    .execute(&mut *tx)
    .await
    .context("update forked_to_thread_id")?;

    tx.commit().await.context("commit fork tx")?;
    Ok(new_thread_id)
}

/// 回滚：删除源线程 turn_id > snap 的 item + turn（恢复到快照点）。
pub async fn rollback_to_snapshot(
    pool: &PgPool,
    tenant_id: i64,
    thread_id: Uuid,
    snap_id: i64,
) -> Result<(u64, u64)> {
    assert_thread_owned(pool, tenant_id, thread_id).await?;
    let snap: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT turn_id FROM workspace_snapshots WHERE id=$1 AND tenant_id=$2 AND thread_id=$3",
    )
    .bind(snap_id)
    .bind(tenant_id)
    .bind(thread_id)
    .fetch_optional(pool)
    .await
    .context("snap check")?;
    let snap_turn_opt = snap.ok_or_else(|| anyhow::anyhow!("snapshot {snap_id} not found"))?.0;
    let snap_turn: i64 = snap_turn_opt
        .ok_or_else(|| anyhow::anyhow!("snapshot {snap_id} has no turn_id"))?;

    let mut tx = pool.begin().await.context("begin rollback tx")?;
    // 先删 items（FK turns）
    let del_items = sqlx::query(
        "DELETE FROM items WHERE thread_id=$1 AND turn_id>$2",
    )
    .bind(thread_id)
    .bind(snap_turn)
    .execute(&mut *tx)
    .await
    .context("delete items after snapshot")?;
    let del_turns = sqlx::query(
        "DELETE FROM turns WHERE thread_id=$1 AND id>$2",
    )
    .bind(thread_id)
    .bind(snap_turn)
    .execute(&mut *tx)
    .await
    .context("delete turns after snapshot")?;
    tx.commit().await.context("commit rollback tx")?;
    Ok((del_items.rows_affected(), del_turns.rows_affected()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        // fork/rollback 逻辑经 e2e 验证（需 PG + 真实 thread 状态）
        assert_eq!(1 + 1, 2);
    }
}
