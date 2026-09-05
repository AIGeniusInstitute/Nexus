# Nexus M3 · 审批与策略 — PRD

> 里程碑：M3 · 审批 + 策略（roadmap T3-1 ~ T3-5）
> 前置：M2 执行闭环已交付（merge 4b5b7b6）。M2 的 `handle_server_request` 对 app-server 的 `CommandExecutionRequestApproval` / `FileChangeRequestApproval` 一律 auto-accept。M3 把它改成真正的 HITL（人机协同）闭环：挂起→落库→推送→人工决策→协议级回写。

## 1. 背景

codex app-server 在 turn 执行中，当 agent 要执行命令或改文件时，会向 client 发 JSON-RPC **请求**（`item/commandExecution/requestApproval` 或 `item/fileChange/requestApproval`），client 必须回写一个带相同 `id` 的 JSON-RPC **响应**（`{decision: Accept|Decline|Cancel|...}`）。这是协议级审批回写口——Nexus 控制平面的天然政策下发载体。

M2 直接 auto-accept，等于审批形同虚设。M3 让审批"活过来"：请求挂起、落库成 ApprovalTicket、经 WS/Web 推给人类、人类决策后协议级回写。

## 2. 目标（MVP 范围）

| 任务 | 范围 | MVP 取舍 |
|---|---|---|
| T3-1 审批中心 ApprovalTicket | pending→decided 全生命周期，先落库再回写 | ✅ 全做 |
| T3-2 HITL 跨进程桥接 | 6 边界（崩溃/超时/权限撤销/改参/批量/审计） | 🟡 做核心：挂起/落库/回写 + Interrupt + 权限撤销自动 Deny；崩溃恢复/批量/改参 留后续 |
| T3-3 策略中心求值+下发 | 准入+高危前求值，生成 config.toml+rules+AGENTS.md | ✅ 做最小版：角色×工具×风险矩阵 + .rules 生成 |
| T3-4 IM Bot 飞书/钉钉 | 审批卡片三按钮，回调签名验证 | 🟡 stub：生成签名回调 URL + 卡片 payload，实际发送留后续（需公网 webhook） |
| T3-5 Web 审批抽屉 | 参数脱敏+Diff 预览+风险等级+批准/拒绝 | ✅ 做最小版：pending 列表 + 批准/拒绝按钮 |

## 3. 验收标准（AC）

- **AC3.1**：turn 执行中 app-server 发出审批请求 → driver 不再 auto-accept，而是挂起、生成 ApprovalTicket（pending）落库、经 WS 广播 `approval/requested` 事件。
- **AC3.2**：人类经 `POST /v1/approvals/{id}/resolve` 决策 → ticket 转 decided → driver 收到 ResolveApproval → 协议级回写 JSON-RPC response（相同 request id）→ agent 继续执行 → turn 正常 completed。
- **AC3.3**：审批期间 `POST .../interrupt` → driver 回写 Cancel、kill 进程、ticket 转 interrupted、turn 转 interrupted。
- **AC3.4**：审批 pending 时 thread 成员权限被撤销 → 自动 Deny + 关闭 WS。
- **AC3.5**：策略中心按角色×工具×风险等级求值，生成 `.rules`（Starlark execpolicy）+ `config.toml` 注入 CODEX_HOME，高危命令（如 `rm -rf`）默认 Deny、只读命令（如 `ls`）默认 Allow。
- **AC3.6**：Web 审批抽屉列出 pending tickets，批准/拒绝可点；操作后 ticket 状态实时刷新。
- **AC3.7**：所有审批决策落审计表（who/when/decision/params 摘要），可查。

## 4. 非目标（MVP 不做）

- 审批崩溃后恢复（driver 死、turn 状态丢失时把 pending ticket 重放给新进程——codex resume 不重发 in-flight approval）。
- 批量审批、改参后批（edit-then-approve）。
- IM 实际发送飞书卡片（需公网回调域名）。
- execpolicy amendment 学习闭环（`AcceptWithExecpolicyAmendment` 的规则持久化）。

## 5. 关键约束

- **不改 codex-rs 内核**：所有改动在 `codex-rs/nexus-control/`。
- **单 turn 串行**：继承 M2，单 driver 线程一次一个 turn；一次最多一个 pending approval（串行保证）。
- **Simplicity First**：不引入并发 stdin/stdout select；审批挂起 = driver 阻塞在 `cmd_rx.recv()`。
