# Nexus M14 任务状态 — Thread Snapshot + Fork + Rollback

## 里程碑
M14 = Thread Snapshot + Fork + Rollback（roadmap M11 T11-4 fork 基础，"Fork-Rollback" M1/M2 遗留项）。
分支：`feat/nexus-m14`，base 含 M13 merge `d0fb04f`。

## 任务清单
| 任务 | 状态 | 说明 |
|---|---|---|
| T14-1 migration | ✅ | workspace_snapshots ALTER ADD tenant_id + forked_to_thread_id + 索引 |
| T14-2 fork.rs 模块 | ✅ | create_snapshot/list_snapshots/fork_from_snapshot/rollback_to_snapshot + assert_thread_owned |
| T14-3 http_server 路由 | ✅ | POST/GET /v1/threads/{id}/snapshots + POST fork + POST rollback |
| T14-4 main.rs 标题 | ✅ | "Nexus M14: serve" |
| T14-5 测试 + e2e | ✅ | cargo test 31/31，e2e AC14.1-14.6 全过 |

## 关键决策
1. **fork 用单一 imported turn**：新 thread 一个 completed turn 携带全部历史 item（content_ref 保留），不逐 turn 复制（避免 turn id 映射复杂度，Simplicity First）
2. **rollback 删 items 再删 turns**（items FK turns，顺序必须）
3. **content_digest 完整性校验**：std DefaultHasher over items.content_ref（非安全，仅检测变更）
4. **不冻结 snapshot 副本**：fork/rollback 时从 live thread 读 ≤ turn_id（MVP 接受时序）
5. **app_server_events 不删**（审计 trail 保留；rollback 只恢复 thread 状态）
6. **fork 用 INSERT...SELECT + ROW_NUMBER()**：seq 从 1 重编，避免 UNIQUE(thread_id,turn_id,seq) 冲突

## 坑
1. `SELECT MAX(id)` 可空返回 NULL→sqlx 元组需 `Option<i64>` 非 `i64`（E0609 no field 0 on Option）
2. items UNIQUE(thread_id,turn_id,seq)：fork 复制用 ROW_NUMBER() OVER (ORDER BY id) 重编 seq，新 thread+新 turn_id 不冲突

## 验证
- cargo check：0 error 0 warning
- cargo test：31/31（M13 30 + fork 1）零回归
- e2e（PG nexus-pg-m4:5434）AC14.1-14.6 全过：
  - AC14.1 workspace_snapshots ALTER 加 tenant_id + forked_to_thread_id（migration incl m14）
  - AC14.2 POST /v1/threads/{id}/snapshots（turn_id=2）→ snapshot_id=1 + content_digest
  - AC14.3 GET snapshots 列表（count=1, turn_id=2, digest）
  - AC14.4 POST fork → 新 thread 含 4 items（turn_id ≤ 2 的全部 item，content_ref 保留）
  - AC14.5 POST rollback → deleted_items=4 + deleted_turns=1（8 items/2 turns → 4 items/1 turn）
  - AC14.6 零回归：M13 KB search（2 hits）+ M12 eval cases（1）
- 不改 codex 内核（全部 nexus-control crate）；不动既有表（workspace_snapshots 用 ALTER）
