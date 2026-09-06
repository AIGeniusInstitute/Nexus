# Nexus M14 技术方案 — Thread Snapshot + Fork + Rollback

## 1. 数据模型（workspace_snapshots 既有表，ALTER 加列）
```sql
ALTER TABLE workspace_snapshots ADD COLUMN IF NOT EXISTS tenant_id BIGINT;
ALTER TABLE workspace_snapshots ADD COLUMN IF NOT EXISTS forked_to_thread_id UUID;
```
既有列：id, thread_id, turn_id, rollout_object_key, content_digest, created_at。

## 2. fork.rs 模块

### create_snapshot(pool, tenant_id, thread_id, turn_id: Option) -> i64
- 校验 thread tenant 归属
- turn_id None → 取最新 turn（`SELECT MAX(id) FROM turns WHERE thread_id=$1`）
- content_digest = std DefaultHasher over items.content_ref（turn_id ≤ snap，ORDER BY id）
- INSERT workspace_snapshots(thread_id, turn_id, content_digest, tenant_id) RETURNING id

### list_snapshots(pool, tenant_id, thread_id) -> Vec<SnapshotRow>
- WHERE tenant_id=$1 AND thread_id=$2

### fork_from_snapshot(pool, tenant_id, thread_id, snap_id) -> Uuid
- 校验 thread + snapshot tenant
- 创建新 thread（title='fork of {src}'，同 tenant + owner）
- 单一 imported turn（status='completed'）在新 thread
- 复制源线程 turn_id ≤ snap.turn_id 的全部 item 到新 thread（新 turn_id，重编 seq，保留 content_ref/content_digest/item_type）
- UPDATE workspace_snapshots SET forked_to_thread_id=新 thread

### rollback_to_snapshot(pool, tenant_id, thread_id, snap_id) -> {deleted_items, deleted_turns}
- 校验 thread + snapshot tenant
- 删 items WHERE thread_id=$1 AND turn_id > snap.turn_id
- 删 turns WHERE thread_id=$1 AND id > snap.turn_id
- 返回删除计数

## 3. HTTP 路由
```
POST /v1/threads/{id}/snapshots                     create_snapshot (body: turn_id?)
GET  /v1/threads/{id}/snapshots                     list_snapshots
POST /v1/threads/{id}/snapshots/{sid}/fork          fork_from_snapshot
POST /v1/threads/{id}/snapshots/{sid}/rollback     rollback_to_snapshot
```
所有路由 AuthUser(c) 提取 tenant_id，校验 thread tenant（与 M11 timeline 同模式）。

## 4. 关键决策
1. **fork 用单一 imported turn**（不复制 turns 行）：新 thread 一个 completed turn 携带全部历史 item（content_ref 保留）。简单 + 给 fork 线程完整上下文。不逐 turn 复制（避免 turn id 映射复杂度，Simplicity First）
2. **rollback 删 items 再删 turns**（items FK turns，顺序必须）
3. **content_digest 完整性校验**：std DefaultHasher（非安全，仅检测变更）
4. **不冻结 snapshot 副本**：fork/rollback 时从 live thread 读 ≤ turn_id（MVP 接受时序；真实冻结需复制 item 到 snapshot_items 表，speculative）
5. **app_server_events 不删**（审计 trail 保留；rollback 只恢复 thread 状态，不擦事件日志）

## 6. 坑预判
- items UNIQUE(thread_id, turn_id, seq)：fork 复制时新 thread+新 turn_id，seq 从 1 重编，不冲突
- rollback 删 turns 时 app_server_events 可能 FK？检查：app_server_events 应无 FK turns（事件日志独立），即使有 ON DELETE CASCADE 也可
