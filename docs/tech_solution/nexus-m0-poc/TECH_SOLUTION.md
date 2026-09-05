# Nexus M0 PoC — 技术方案

> 需求：nexus-m0-poc · 分支：`feat/nexus-m0-poc` · worktree：`.worktrees/feat/nexus-m0-poc`
> 日期：2026-09-06 · 关联 PRD：`docs/prd/nexus-m0-poc/PRD.md`

## 1. 架构决策

### 1.1 控制面 crate 位置与语言

- **语言**：Rust（与 Harness 同语言，直接复用 `codex-app-server-protocol` 类型）
- **位置**：`codex-rs/nexus-control/`，作为 codex-rs workspace 新 member（在 `codex-rs/Cargo.toml` 加一行）
- **理由**：共享构建缓存（106 crate 已就位，增量编译快）；类型引用零配置；符合 AGENTS.md"新概念放新 crate 不膨胀 core"

### 1.2 app-server 集成传输：stdio 子进程

- 不用 `InProcessAppServerClient`（in-process，无法 kill/restart）
- 用 **stdio 子进程**：nexus-control 起 `codex app-server --stdio` 子进程，经 stdin/stdout JSON-RPC（JSONL）通信
- **理由**：Harness 独立进程，可 kill → 直接验证 T0-3（resume）；符合控制面/执行面分离

参考实现：`codex-rs/app-server-test-client/src/lib.rs` 的 `CodexClient`（Stdio transport，已验证可用）。

### 1.3 事件落库：SQLite

- PoC 用 SQLite（`rusqlite` crate），M1 迁 Postgres+RLS
- 幂等键：`thread_id + turn_id + item_seq`

## 2. Crate 结构

```
codex-rs/nexus-control/
├── Cargo.toml
└── src/
    ├── lib.rs          # 公开模块
    ├── main.rs         # CLI 入口（nexus-control poc）
    ├── stdio_client.rs # app-server stdio JSON-RPC client
    ├── event_store.rs  # SQLite 事件落库
    └── proto.rs        # 协议类型 re-export + helper
```

### Cargo.toml 依赖

```toml
[package]
name = "nexus-control"
version = "0.1.0"
edition = "2024"

[dependencies]
codex-app-server-protocol = { workspace = true }  # ClientRequest/ClientNotification/ServerNotification 类型
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "process", "io-util", "io-std", "sync", "time"] }
serde = { workspace = true }
serde_json = { workspace = true }
rusqlite = { version = "0.32", features = ["bundled"] }  # SQLite，bundled 免系统依赖
anyhow = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

## 3. 模块设计

### 3.1 stdio_client.rs — app-server 客户端

```rust
pub struct AppServerProcess {
    child: tokio::process::Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl AppServerProcess {
    /// 起 `codex app-server --stdio` 子进程
    pub async fn spawn(codex_bin: &Path, codex_home: &Path) -> Result<Self>;
    
    /// 发 JSON-RPC request，等 response（按 id 匹配），跳过期间的通知
    pub async fn request<R: DeserializeOwned>(&mut self, method: &str, params: Value) -> Result<R>;
    
    /// 读下一条 ServerNotification（或 response）
    pub async fn next_event(&mut self) -> Result<Option<ServerEvent>>;
    
    /// 发 notification（initialized）
    pub async fn notify(&mut self, method: &str, params: Value) -> Result<()>;
    
    /// kill 子进程
    pub fn kill(&mut self);
}
```

**JSON-RPC 帧格式**：每行一个 JSON 对象（`{"id":N,"method":"...","params":{...}}` 请求；`{"id":N,"result":{...}}` 响应；`{"method":"...","params":{...}}` 通知）。

### 3.2 event_store.rs — SQLite 落库

```sql
CREATE TABLE events (
    thread_id  TEXT NOT NULL,
    turn_id    TEXT NOT NULL,
    item_seq   INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    payload    TEXT NOT NULL,      -- JSON
    ts         INTEGER NOT NULL,   -- Unix ms
    PRIMARY KEY (thread_id, turn_id, item_seq)
);
```

```rust
pub struct EventStore { conn: rusqlite::Connection }
impl EventStore {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn upsert_event(&self, thread_id: &str, turn_id: &str, seq: i64, etype: &str, payload: &str) -> Result<()>;
    pub fn max_seq(&self, thread_id: &str, turn_id: &str) -> Result<i64>;
    pub fn count(&self) -> Result<i64>;
}
```

### 3.3 main.rs — PoC 驱动

PoC 命令：`nexus-control poc --codex-bin <path> --codex-home <dir>`

执行序列：
1. spawn app-server 子进程
2. `initialize` request → 收 server info → `initialized` notify
3. `thread/start` → 得 thread_id
4. `turn/start`（输入："Please run `ls` and report the files"）→ 得 turn_id
5. loop `next_event`：每条事件落库 + 打印；遇 `turn/completed` 退出
6. （resume 演示）kill 子进程 → 重 spawn → `thread/resume(threadId)` → 继续收事件验证 seq 连续

## 4. API 调用序列（基于 app-server README）

```
client → server:  initialize {client_info, capabilities}
server → client:  result {userAgent, codexHome, platformFamily, platformOs}
client → server:  initialized (notification)
client → server:  thread/start {title?}
server → client:  result {thread{id,...}} + thread/started (notification)
client → server:  turn/start {threadId, input}
server → client:  result {turn{id,...}} + turn/started (notification)
server → client:  item/started, item/completed, item/agentMessage/delta, ... (stream)
server → client:  turn/completed {turn, usage}
```

resume：
```
client → server:  thread/resume {threadId, ...}
server → client:  result + 继续事件流（item_seq 从断点）
```

## 5. 关键风险与缓解

| 风险 | 缓解 |
|---|---|
| codex-app-server 二进制未构建 | 先 `cargo build -p codex-app-server --bin codex-app-server`（或 codex bin） |
| JSON-RPC 帧解析错误（多行/转义） | 用 serde_json 逐行 Deserializer；参考 test-client CodexClient |
| 协议类型不匹配（camelCase） | 复用 app-server-protocol 的 ThreadStartParams/TurnStartParams，不自造 |
| resume 需要 rollout 文件 | codex 默认 ~/.codex/sessions/ 落 rollout；保持 codex_home 一致即可恢复 |

## 6. 构建与运行

```shell
# 在 worktree 内
cd ~/Nexus/.worktrees/feat/nexus-m0-poc
# 1. 构建 codex-app-server 二进制（首次慢，~10min）
cargo build --manifest-path codex-rs/Cargo.toml -p codex-app-server
# 2. 构建 nexus-control
cargo build --manifest-path codex-rs/Cargo.toml -p nexus-control
# 3. 跑 PoC
./codex-rs/target/debug/nexus-control poc \
  --codex-bin ./codex-rs/target/debug/codex \
  --codex-home /tmp/nexus-poc-home
```

## 7. 测试策略

- 单元：event_store 幂等/缺口测试（rusqlite 内存库）
- 集成：spawn app-server → thread/start → turn/start → 收事件 → 落库 → count 校验
- resume：kill → 重 spawn → thread/resume → seq 连续
- 每个 AC 对应一个测试用例，截图/日志存 test_report

## 8. 非目标（PoC 不做）

- 多租户、RLS、Postgres（M1+）
- 审批 HITL（M3）
- 计费、配额（M4）
- K8s（M2，PoC 用本地进程）
- UI（CLI 即可）
