# Nexus M3 — 审批与策略 · 任务执行状态

> 分支 `feat/nexus-m3` · 里程碑 M3（Approval & Policy）· 2026-09-06

## 里程碑目标

将 app-server 的**自动接受审批**转变为**人机协作闭环**：surface → park → resolve → 协议级回写 → turn 完成。
配套策略中心（Starlark rules 生成）与 Web 审批抽屉。

## 任务清单

| 任务 | 主题 | 状态 |
|------|------|------|
| T3-1 | ApprovalTicket 生命周期（schema + 审计） | ✅ |
| T3-2 | HITL 桥接（driver park / resolve 回写） | ✅ |
| T3-3 | 策略中心（policies + Starlark 生成） | ✅ |
| T3-4 | IM Bot（桩，留 M4） | ⏭ stub |
| T3-5 | Web 审批抽屉 | ✅ |
| T3-6 | 集成验收（e2e 3 例） | ✅ |
| T3-7 | WS 撤销中断（AC3.4） | ✅ |

## 执行记录

### T3-1 ApprovalTicket 生命周期
- 迁移 `20260906000003_m3_approval.sql`：ALTER approval_tickets（tenant_id/kind/status/item_id/jsonrpc_id/command/cwd/reason/raw_params）+ approval_audit + policies（唯一索引 uq_policies_role_kind_pattern + 7 条种子）。
- `db.rs` 挂载 M3_MIGRATION_SQL 执行。
- ✅ 完成。

### T3-2 HITL 桥接（核心）
- **死锁解法**：M2 的 `runtime: Arc<Mutex<RuntimeHandle{cmd_tx,event_rx}>>` 中 turn_start 持锁 drain event_rx，resolve 若也需锁 → 死锁。拆分为 `runtime_cmd: Sender<DriverCommand>`（Clone，无锁，AppState 顶层）+ `runtime_events: Arc<Mutex<UnboundedReceiver<TurnEvent>>>`（仅 turn_start 读）。
- **Driver park 逻辑**：遇 ApprovalRequest → 发 approval/requested TurnEvent + 记 ParkedApproval{approval_id,jsonrpc_id,kind} → park on cmd_rx.recv()；收到 ResolveApproval → 经 respond_approval 写回 JSON-RPC Response（id 匹配）→ 继续 drain；收到 Interrupt → 写 Cancel + 发 approval/interrupted + break。
- **SIMULATE 测试模式**（`NEXUS_SIMULATE_APPROVAL=1`）：driver 注入合成 approval/requested → park → resolve 时发合成 item+turn/completed，无需真实模型即可端到端验证 HITL 桥。
- **approval_id 碰撞修复**：driver 的 next_approval_id 重启归 1 与已有 DB 行碰撞（INSERT ON CONFLICT DO NOTHING 静默跳过）。修复：spawn() 取 start_approval_id = `SELECT COALESCE(MAX(id),0)+1 FROM approval_tickets`（main.rs 查询）。验证：ids 现为 6,7,8（续 DB max），无碰撞。
- ✅ 完成。

### T3-3 策略中心
- `policy.rs`：PolicyDecision{Allow,Prompt,Deny}；evaluate() 按 priority desc + glob pattern_match；risk_of() 启发式（rm -rf/sudo/mkfs/dd/curl|sh→high，rm/mv/chmod→medium，else low）；generate_rules() → Starlark forbid()/allow()。
- 3 单测通过。
- ✅ 完成。

### T3-4 IM Bot
- 桩，留 M4（需 IM sender→userId 解析器，与平台 multi-user-collaboration 的 webhook 同源）。
- ⏭ 延后。

### T3-5 Web 审批抽屉
- `api.ts`：Approval 接口 + listApprovals/resolveApproval。
- `App.tsx`：ApprovalsPage（列表 + 批准/拒绝按钮 + 刷新）+ nav 切换 会话/审批。
- tsc --noEmit exit 0，vite build 31 modules。
- ✅ 完成。

### T3-6 集成验收
- e2e 3 例（approve/deny/interrupt）全过：ticket 状态正确（approved/denied/interrupted）+ audit 记录 + turn 状态正确（completed/completed/interrupted）。
- ✅ 完成。

### T3-7 WS 撤销中断
- `ws.rs`：权限撤销时先 `runtime_cmd.send(Interrupt)`（driver 写 Cancel + ticket→interrupted）再发 `{"event":"revoked"}` + close。
- e2e interrupt 例验证：ticket interrupted + audit interrupted:cancelled + turn interrupted。
- ✅ 完成。

## 验证结果

| 项 | 结果 |
|----|------|
| cargo check | 0 error 0 warning |
| cargo test | 17/17 通过 |
| web tsc --noEmit | exit 0 |
| web vite build | 31 modules 成功 |
| e2e approve | ticket=approved turn=completed ✅ |
| e2e deny | ticket=denied turn=completed ✅ |
| e2e interrupt | ticket=interrupted turn=interrupted ✅ |

## 退出条件

✅ 全部任务完成，e2e 3 例通过，无回归。M3 交付。
