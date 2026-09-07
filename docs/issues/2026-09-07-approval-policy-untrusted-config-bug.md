# approval_policy = "untrusted" 配置已被 codex 移除

## 1. 用户现象

在 macOS 本机执行 `./deploy/deploy.sh` 部署 Nexus 后，`nexus-control` 容器健康检查通过、`/health` 返回 `ok`、登录接口能返回 JWT，但 **agent 无法真正运行**：控制面日志持续报错，driver 池的 eager 预热（warm pool）反复失败。

外部表现：控制面能起、能登录，但一旦发起一个真正的 agent 任务（`turn/start`），底层 codex 引擎子进程启动即失败，任务无法推进。

## 2. 问题描述

`nexus-control` 启动后，driver 线程的 M15 预热逻辑会 eager 拉起 `codex app-server` 子进程（`runtime.rs` 的 `driver_loop` → `AppServerProcess::spawn` → `initialize()`）。该子进程在读取 `CODEX_HOME/config.toml` 时，因配置里存在已废弃的 `approval_policy = "untrusted"` 项而被 codex 拒绝，进程启动即退出。

容器日志证据：

```
Error: approval_policy = "untrusted" is no longer supported; remove this setting
warm: eager init failed, lazy fallback on first turn error=warm app-server initialize
```

## 3. 根因

`codex` 上游在较新版本中**移除了 config.toml 里的 `approval_policy = "untrusted"` 取值**（对应内部枚举 `AskForApproval::UnlessTrusted`，它已从"可在配置中设置"降级为"仅内部项目信任推导"）。

- `codex-rs/protocol/src/protocol.rs:984` 定义 `enum AskForApproval`，其中 `UnlessTrusted`（serde 别名 `"untrusted"`）仍存在但不再允许出现在 config.toml。
- `codex-rs/core/src/config/mod.rs:206-207` 定义了专用错误 `UnsupportedUntrustedApprovalPolicyError`，错误文案即 `approval_policy = "untrusted" is no longer supported; remove this setting`。
- `codex-rs/core/src/config/mod.rs:3640` 在配置校验时显式拒绝：
  ```rust
  if cfg.approval_policy == Some(AskForApproval::UnlessTrusted) {
      return Err(std::io::Error::new(ErrorKind::InvalidData, UnsupportedUntrustedApprovalPolicyError));
  }
  ```

而 Nexus 控制面的配置生成器 `codex-rs/nexus-control/src/execpolicy_rules.rs::write_config_toml()` 仍写死了这一行：

```toml
approval_policy = "untrusted"
```

于是每次 nexus-control 写入 `config.toml`，都会让 codex 子进程在启动时被拒绝。命令的允许/拒绝本应完全由 `execpolicy` 的 `.rules` 文件（`rm` Forbidden / `ls` Allow）承担，`approval_policy` 这条是遗留的、已失效的配置。

## 4. 复现路径

1. 在 macOS（Apple Silicon）上跑 `./deploy/build-linux-binaries.sh` 编译出 aarch64 Linux 二进制，再 `./deploy/deploy.sh` 拉起 `nexus-control` + `nexus-postgres`。
2. 等容器健康后 `docker logs nexus-control`，观察 `warm: eager init failed` 与 `approval_policy = "untrusted" is no longer supported`。
3. 或直接查看控制面生成的配置：
   ```bash
   docker exec nexus-control cat /app/.codex/config.toml
   # 其中含 approval_policy = "untrusted"
   ```

## 5. 诊断方法

```bash
# 1) 定位错误来源
docker logs nexus-control 2>&1 | grep -iE "approval|eager|warm|untrusted"

# 2) 查看运行时生成的 config.toml
docker exec nexus-control cat /app/.codex/config.toml

# 3) 确认 codex 拒绝该配置（有 bug 时：进程立即报错退出）
docker exec nexus-control sh -c 'CODEX_HOME=$(mktemp -d) timeout 3 /usr/local/bin/codex app-server 2>&1 | head'

# 4) 用修复后的 config（去掉 approval_policy 行）做 initialize 握手验证
docker exec -i nexus-control sh -c '
T=$(mktemp -d)
cat > "$T/config.toml" <<EOF
model = "deepseek-v4-pro"
model_provider = "nexus-gateway"
sandbox_mode = "danger-full-access"

[model_providers.nexus-gateway]
name = "Nexus Gateway"
base_url = "http://127.0.0.1:34897/v1"
experimental_bearer_token = "nexus-gateway-f07ffd54da1e475e8cb2f5bfb5cd52a8"
wire_api = "responses"
EOF
CODEX_HOME="$T" /usr/local/bin/codex app-server 2>err.log <<JSON
{"id":"init-1","method":"initialize","params":{"clientInfo":{"name":"nexus-control","title":"Nexus Control","version":"0.1.0"},"capabilities":{"experimentalApi":true,"requestAttestation":false,"mcpServerOpenaiFormElicitation":false}}}
{"method":"initialized"}
JSON
grep -iE "approval|error" err.log || echo "OK: 无 approval_policy 错误"
'
```

修复后的握手应返回类似：

```json
{"id":"init-1","result":{"userAgent":"nexus-control/0.0.0 (...)","codexHome":"/tmp/...","platformFamily":"unix","platformOs":"linux"}}
```

## 6. 修复方案

选型理由：命令级管控已由 execpolicy `.rules`（`rm` Forbidden / `ls` Allow）负责；`approval_policy` 的 `untrusted` 值已被上游移除，保留它只会让 codex 启动失败。默认行为（`OnRequest`，由模型决定何时请求审批）已满足需求，故**直接删除该配置行**即可，无需新增替代项。

diff（`codex-rs/nexus-control/src/execpolicy_rules.rs`）：

```diff
 model = "{model}"
 model_provider = "nexus-gateway"
-# M9: untrusted approval policy — commands require approval unless an
-# explicit execpolicy rule allows them, so real CommandExecutionRequestApproval
-# flows can be exercised end-to-end (not just via SIMULATE).
-approval_policy = "untrusted"
+# Approval policy is left unset: codex removed the `untrusted` value
+# (AskForApproval::UnlessTrusted) from config.toml — it is now an internal
+# policy derived from project trust, and defaults to OnRequest (the model
+# decides when to ask). Command allow/deny is enforced by execpolicy `.rules`.
 sandbox_mode = "danger-full-access"
```

## 7. 处理卡住的状态

若容器已在运行且 config.toml 已写入错误的 `approval_policy` 行：

```bash
# 旧二进制会每次启动都重新生成错误 config，需先替换二进制再重启。
# 若只想立即验证，可临时去掉该行后重启，但重启仍会复写 —— 必须用修复后的 nexus-control 二进制。
docker compose --env-file .env build && docker compose --env-file .env up -d
```

## 8. 经验沉淀 / 预防

- **codex 上游会以"移除配置取值"的方式做破坏性变更**：`approval_policy` 这类枚举值可能被降级为内部推导，但序列化别名仍保留。控制面在生成 `config.toml` 时不应写死上游枚举值，而要遵循"最小配置"原则——只写业务确实依赖的项。
- **命令管控的单一事实源是 execpolicy `.rules`**，不应再依赖 `approval_policy` 的 `untrusted` 语义。
- 建议加一条巡检：容器起后校验 `codex app-server` 能完成 `initialize` 握手（如上面的 smoke 测试），失败即告警，避免"控制面健康但 agent 引擎起不来"的假健康。
