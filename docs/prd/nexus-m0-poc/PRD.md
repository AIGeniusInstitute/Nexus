# Nexus M0 PoC — 需求 PRD

> 需求编号：nexus-m0-poc · 阶段：① PoC（roadmap M0）
> 工作分支：`feat/nexus-m0-poc` · 日期：2026-09-06
> 控制面 crate：`codex-rs/nexus-control/`

## 1. 背景与目标

Nexus 要在 OpenAI Codex Harness（codex-rs）之上构建企业级 Agent 平台。M0 PoC 的使命是**验证三大可行性假设**，为后续 MVP 投入提供证据：

| # | 假设 | 验证手段 |
|---|---|---|
| H1 | app-server 可作为长会话集成面，且会话可跨进程恢复 | 起 codex-app-server（unix socket）→ thread/start → turn/start → 杀进程 → 重连 thread/resume → 事件 seq 连续无丢失 |
| H2 | execpolicy 规则可按租户/角色下发并生效 | 注入 .rules 文件，`rm -rf /` 被 Forbidden、`ls` 被 Allow |
| H3 | 三层沙箱（容器+OS+网络）可生效 | Pod 内 Landlock/seccomp 可用、出站仅白名单、无长期密钥 |

**PoC 非目标**：多租户、审批 HITL、计费、生产可用 UI。仅验证骨架可跑通。

## 2. 功能点清单

| FP | 功能点 | 对应任务 | 优先级 |
|---|---|---|---|
| FP1 | app-server 集成：Nexus 控制面经 unix socket 与 codex-app-server 双向 JSON-RPC 通信 | T0-1 | P0 |
| FP2 | 事件流落库：消费 ServerNotification，按 thread_id+turn_id+item_seq 幂等写入本地库 | T0-2 | P0 |
| FP3 | thread/resume 恢复：杀 app-server 进程后重启，resume 后事件 seq 连续 | T0-3 | P0 |
| FP4 | execpolicy 下发：注入 Starlark 规则集，命令 allow/deny 生效 | T0-4 | P0 |
| FP5 | 三层沙箱容器层：K8s Pod + Seccomp/AppArmor + NetworkPolicy 默认全禁 | T0-5 | P0 |
| FP6 | 沙箱启动自检：5 项自检脚本，不过则禁止调度 | T0-6 | P0 |
| FP7 | Model Gateway 代理：沙箱出站只到 Gateway，令牌按任务绑定 | T0-7 | P0 |
| FP8 | M0 集成验收：端到端跑通一任务 | T0-8 | P0 |

## 3. 功能点详述与验收标准

### FP1 app-server 集成（T0-1）

**需求**：新建 Rust crate `nexus-control`，实现 app-server 客户端：
- 启动 codex-app-server 子进程，`--listen unix://<sockpath>`
- 经 unix socket 发 JSON-RPC：`initialize` → `thread/start` → `turn/start`
- 收 ServerNotification 事件流（thread/run_turn_started, item/*, turn/finished 等）
- 打印事件到 stdout（PoC 可视化）

**验收标准**：
- AC1.1 `cargo build -p nexus-control` 成功
- AC1.2 运行 `nexus-control poc` 能起 app-server 子进程并建立 unix socket 连接
- AC1.3 `initialize` 握手返回 server info（含版本号）
- AC1.4 `thread/start` 返回非空 thread_id
- AC1.5 `turn/start` 后收到至少 1 条 ServerNotification 事件
- AC1.6 Ctrl-C 优雅关闭子进程，socket 文件清理

### FP2 事件流落库（T0-2）

**需求**：nexus-control 消费事件流，写入本地 SQLite（PoC 用 SQLite，M1 迁 Postgres）：
- 表 `events(thread_id, turn_id, item_seq, event_type, payload_json, ts)`
- 幂等：`thread_id+turn_id+item_seq` 唯一，重复事件 INSERT OR IGNORE
- 缺口检测：记录期望 max_seq，新事件 seq > max+1 则标记 gap

**验收标准**：
- AC2.1 一轮 turn 产生的事件全部入库，行数 = 收到事件数
- AC2.2 重复投递同一事件，库中不产生重复行（幂等）
- AC2.3 `SELECT count(*) FROM events` 与事件流计数一致

### FP3 thread/resume 跨进程恢复（T0-3）

**需求**：验证 H1——会话不随进程死亡丢失：
- 第一轮 turn 跑到一半，kill app-server 子进程
- 重启 app-server，发 `thread/resume`（带原 thread_id）
- 继续收事件，item_seq 从断点继续，无丢失/重复

**验收标准**：
- AC3.1 kill 前已落库的事件在重启后仍在库中
- AC3.2 resume 后新事件 seq 严格 > kill 前最大 seq
- AC3.3 无重复事件入库（幂等生效）

### FP4 execpolicy 规则下发（T0-4）

**需求**：验证 H2——策略可下发：
- 生成 execpolicy `.rules` 文件：`rm -rf /` → Forbidden，`ls` → Allow
- 注入 app-server config（execpolicy 路径）
- turn 内尝试 `rm -rf /` → 被拦截（工具返回拒绝）；`ls` → 放行

**验收标准**：
- AC4.1 `rm -rf /` 命令被 execpolicy 判定 Forbidden，不执行
- AC4.2 `ls` 命令被判定 Allow，执行成功
- AC4.3 判定日志可见（哪个规则命中）

### FP5 三层沙箱容器层（T0-5）

**需求**：验证 H3 第一层——容器隔离：
- Docker/K8s Pod（PoC 用 Docker 容器模拟，M2 上 K8s）
- Seccomp profile 限制 syscall；NetworkPolicy 用 iptables/默认 deny 出站
- 容器内跑 codex-app-server

**验收标准**：
- AC5.1 容器内 `ping 8.8.8.8` 被拒绝（出站禁）
- AC5.2 容器内 app-server 可达 Model Gateway（白名单放行）
- AC5.3 容器以非 root 运行

### FP6 沙箱启动自检（T0-6）

**需求**：5 项自检脚本，启动时执行，任一不过则拒绝调度：
1. Landlock/seccomp/Seatbelt 可用性
2. 出站仅白名单两地址
3. 镜像无长期密钥（env/文件扫描）
4. 只读 rootfs + 非 root
5. 资源限额（cgroup CPU/MEM/PID）生效

**验收标准**：
- AC6.1 5 项全过 → 返回 0，允许调度
- AC6.2 故意破坏一项（如开放出站）→ 返回非 0，拒绝调度

### FP7 Model Gateway 代理（T0-7）

**需求**：验证模型调用经 Gateway：
- 起一个简易 HTTP 代理（PoC 用 Rust hyper 或简易 echo），转发到真实模型 API
- app-server config 指向 Gateway 地址 + 短期令牌
- 沙箱出站只到 Gateway

**验收标准**：
- AC7.1 app-server 模型请求经 Gateway 转发
- AC7.2 令牌校验：错令牌被拒
- AC7.3 Gateway 记录计量（prompt/output token 数）

### FP8 M0 集成验收（T0-8）

**需求**：端到端串联：
- 提交任务（"列出当前目录文件"）
- 经 FP1 app-server → FP7 Model Gateway 采样 → FP4 execpolicy 放行 `ls` → FP5 沙箱执行 → FP2 事件落库 → 产物返回
- kill 重启 resume（FP3）

**验收标准**：
- AC8.1 全链路无人工介入跑通
- AC8.2 产物（ls 输出）可见
- AC8.3 resume 后状态一致

## 4. 测试用例

| 用例ID | 功能点 | 步骤 | 预期 | 对应 AC |
|---|---|---|---|---|
| TC-01 | FP1 | 起 nexus-control，发 initialize | server info 返回 | AC1.3 |
| TC-02 | FP1 | thread/start | 非空 thread_id | AC1.4 |
| TC-03 | FP1 | turn/start（"hello"） | ≥1 条事件 | AC1.5 |
| TC-04 | FP2 | 跑一轮 turn，查库 | 行数=事件数 | AC2.1 |
| TC-05 | FP2 | 重复投递同一事件 | 无重复行 | AC2.2 |
| TC-06 | FP3 | turn 中 kill，重启 resume | seq 连续 | AC3.1-3 |
| TC-07 | FP4 | 注入规则，试 rm -rf / | Forbidden | AC4.1 |
| TC-08 | FP4 | 试 ls | Allow | AC4.2 |
| TC-09 | FP5 | 容器内 ping | 拒绝 | AC5.1 |
| TC-10 | FP6 | 自检全过 | 返回 0 | AC6.1 |
| TC-11 | FP6 | 破坏出站，自检 | 拒绝调度 | AC6.2 |
| TC-12 | FP7 | 经 Gateway 调模型 | 转发成功 | AC7.1 |
| TC-13 | FP8 | 端到端 ls 任务 | 产物可见 | AC8.1-2 |

## 5. 约束与非目标

- **不改 codex-rs 内核**：仅在 codex-rs/ 加新 crate `nexus-control/`，不改现有 crate 源码（Cargo.toml 加 member 除外）
- **PoC 用 SQLite**：M1 再迁 Postgres+RLS
- **PoC 用 Docker**：M2 再上 K8s
- **无 UI**：CLI 可视化即可
- **无多租户**：单租户单任务
