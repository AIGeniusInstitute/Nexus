# Nexus M14 PRD — Thread Snapshot + Fork + Rollback

## 背景
roadmap M11 T11-4「多 Agent 协作编排」（主从/流水线/专家路由 + fork + Guardian 审查）。`fork` 是其基础原语。同时 "Fork-Rollback" 是 M1/M2 明确遗留项（初始 migration 已建空壳 `workspace_snapshots` 表，从未接线）。

## 目标
为 Nexus 控制面补齐**线程快照 + 分叉 + 回滚**：在某个 turn 后拍快照 → 从快照分叉出新线程（携带此前全部 item 上下文）做实验 → 或回滚线程到快照点（丢弃之后的 turn/item）。这是多 Agent 协作编排的 fork 基础。

## 范围（MVP）
1. **创建快照**：POST /v1/threads/{id}/snapshots — 在指定 turn（默认最新 completed turn）拍快照，记录 turn_id 边界 + content_digest（完整性校验）
2. **列表快照**：GET /v1/threads/{id}/snapshots
3. **分叉**：POST /v1/threads/{id}/snapshots/{sid}/fork — 创建新线程，复制源线程 turn_id ≤ 快照点的全部 item 到新线程（单一 imported turn，保留 content_ref），返回新 thread_id
4. **回滚**：POST /v1/threads/{id}/snapshots/{sid}/rollback — 删除源线程 turn_id > 快照点的 item + turn（恢复到快照点）

## 非目标（留扩展）
- 主从/流水线/专家路由编排模式（speculative，留真实场景驱动）
- Guardian 审查 gate（留扩展）
- 跨线程 item id 保持（fork 用新 id，content_ref 保留即上下文保留）
- snapshot 时刻冻结副本（fork/rollback 时从 live thread 读 ≤ turn_id，MVP 接受快照后线程继续写入的时序）

## 验收标准
- AC14.1 workspace_snapshots ALTER 加 tenant_id + forked_to_thread_id（migration incl m14）
- AC14.2 POST /v1/threads/{id}/snapshots 创建快照（turn_id + content_digest）
- AC14.3 GET /v1/threads/{id}/snapshots 列表（tenant 隔离）
- AC14.4 POST fork → 新 thread 含源线程 ≤ 快照点的全部 item（content_ref 保留）
- AC14.5 POST rollback → 源线程 turn_id > 快照点的 item/turn 被删除
- AC14.6 零回归：M13 KB + M12 eval + M11 timeline + M10 audit 仍工作

## 约束
- 不改 codex 内核（全部 nexus-control crate）
- 不动既有表（workspace_snapshots 用 ALTER；不删旧列）
