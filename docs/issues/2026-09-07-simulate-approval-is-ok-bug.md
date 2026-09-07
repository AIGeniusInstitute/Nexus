# Issue: NEXUS_SIMULATE_APPROVAL `is_ok()` 判断 bug — `=0` 仍走 SIMULATE 合成模式，真实模型路径从未走到

**发现时间**：2026-09-07
**发现场景**：用户要求"测试 Agent 对话功能"，部署后用真实模型（glm-4-flash）测对话，发现 turn 始终返回 mock 数据（model=nexus-gateway-mock, tokens 10/20, command="rm -rf /tmp/nexus-sim"）。

## 1. 用户现象

部署 Nexus 最新代码（含 531c0ac 修复 approval_policy），配置真实模型（智谱 glm-4-flash + ZHIPU_API_KEY），关闭 SIMULATE（`.env` 删除 `NEXUS_SIMULATE_APPROVAL` 或设 `=0`），发起 Agent 对话。期望：真实模型回复。实际：turn 返回的 `model` 恒为 `nexus-gateway-mock`、`input_tokens=10/output_tokens=20`（mock 固定值）、approval 的 command 恒为 `rm -rf /tmp/nexus-sim`（SIMULATE 默认合成命令）——**真实模型路径从未被走到**。

## 2. 问题描述

`codex-rs/nexus-control/src/runtime.rs:315` 判断是否走 SIMULATE 合成模式：

```rust
let simulate_approval = std::env::var("NEXUS_SIMULATE_APPROVAL").is_ok();
```

`is_ok()` 只检查环境变量**是否存在**，不检查**值**。而 `deploy/docker-compose.yml` 的 environment 块显式声明：

```yaml
NEXUS_SIMULATE_APPROVAL: ${NEXUS_SIMULATE_APPROVAL:-0}
```

即使 `.env` 删除了该变量、或设 `=0`，docker-compose 仍向容器注入 `NEXUS_SIMULATE_APPROVAL=0`（env 存在、值为 0）→ `is_ok()` 返回 `true` → **永远卡在 SIMULATE 合成模式**。SIMULATE 分支（runtime.rs:452）注入合成 approval + 合成 item/completed + mock usage（in=10/out=20/model=nexus-gateway-mock），完全绕过真实模型流（gateway upstream passthrough 不被读取，driver 不 drain 真实 app-server 事件）。

后果：M8（真实模型联调）/M9（function calling 双向协议转换）之后所有"真实模式"部署，只要 docker-compose 声明了该 env，实际都跑在 SIMULATE 上——真实对话从未发生。

## 3. 根因

**代码层面**：`std::env::var(name).is_ok()` 语义是"变量已设置（不论值）"，而非"变量为真"。对于布尔开关 env，必须检查值。

**外部依据**：Rust std 文档 `std::env::VarError::NotPresent`——`is_ok()` 仅在变量存在时返回 true，空串和 "0" 都算存在。

**为何长期未暴露**：M8/M9 e2e 验证用的是 `NEXUS_SIMULATE_APPROVAL=1`（SIMULATE 零回归）+ 单独的真实模型直连测试（不经 docker-compose env 声明），所以"=0 关 SIMULATE"这条路径从未被 docker 部署验证过。M21 引入 `NEXUS_SIMULATE_JUDGE` 时在 docker-compose.yml 加了 `${...:-0}` 模式声明，使该 env 在容器里永远存在。

## 4. 复现路径

```bash
cd ~/Nexus
# .env 配真实模型但关 SIMULATE
grep -E 'NEXUS_MODEL=|NEXUS_UPSTREAM_MODEL_URL|NEXUS_SIMULATE_APPROVAL' deploy/.env
# NEXUS_MODEL=glm-4-flash
# NEXUS_UPSTREAM_MODEL_URL=https://open.bigmodel.cn/api/paas/v4
# NEXUS_SIMULATE_APPROVAL=0   ← 设 0 期望关闭

./deploy/deploy.sh --no-build   # 重载 env
# login + create thread + start turn
TOK=$(curl -s -X POST http://localhost:8765/v1/auth/login -H 'Content-Type: application/json' \
  -d '{"email":"admin@nexus.local","password":"admin123"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
TID=$(curl -s -X POST http://localhost:8765/v1/threads -H "Authorization: Bearer $TOK" \
  -H 'Content-Type: application/json' -d '{"title":"repro"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
curl -s -X POST "http://localhost:8765/v1/threads/$TID/turns" -H "Authorization: Bearer $TOK" \
  -H 'Content-Type: application/json' -d '{"prompt":"你好"}' &  # 后台（SIMULATE 会 park 审批）
sleep 3
# 仍出现合成审批 command="rm -rf /tmp/nexus-sim" → 证明在 SIMULATE
docker exec nexus-postgres psql -U nexus -d nexus -t -c \
  "SELECT model, input_tokens, output_tokens FROM turns ORDER BY id DESC LIMIT 1;"
# 修复前：model=nexus-gateway-mock, 10, 20  ← 仍是 mock
```

## 5. 诊断方法

```bash
# 1. 确认容器内 env 值（不论 .env 怎么设，compose 声明让它永远存在）
docker exec nexus-control printenv NEXUS_SIMULATE_APPROVAL
# 0   ← 值是 0，但 is_ok() 仍 true

# 2. 确认 driver 走了 SIMULATE 分支（日志）
docker logs nexus-control 2>&1 | grep "simulated approval resolved"
# 有输出 → 确认走了 SIMULATE 合成分支

# 3. 确认 turn 落库的是 mock
docker exec nexus-postgres psql -U nexus -d nexus -t -c \
  "SELECT model FROM turns ORDER BY id DESC LIMIT 1;"
# nexus-gateway-mock → 真实模型未被调用
```

## 6. 修复方案

`codex-rs/nexus-control/src/runtime.rs:315`：

```diff
-    let simulate_approval = std::env::var("NEXUS_SIMULATE_APPROVAL").is_ok();
+    // Value-checked (not mere presence): `=0` or unset must mean OFF. The old
+    // `.is_ok()` treated any presence — including `=0` — as on, so docker-compose
+    // declaring `NEXUS_SIMULATE_APPROVAL: ${...:-0}` kept SIMULATE on permanently
+    // and the real-model path was never reached.
+    let simulate_approval = std::env::var("NEXUS_SIMULATE_APPROVAL")
+        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
+        .unwrap_or(false);
```

**选型理由**：
- 值检查（`=="1"||=="true"`）是布尔 env 开关的标准判断方式，与 `NEXUS_SIMULATE_JUDGE`、`NEXUS_DISABLE_WARM_POOL` 等同类开关一致。
- `unwrap_or(false)`：env 不存在 → false（真实模式），符合"默认非 SIMULATE"语义。
- 不改 docker-compose.yml：`${...:-0}` 声明保留（显式默认 0 可读性好），修复后 `=0` 正确表示关闭。
- 不引入 `parse_bool` 依赖：两个值够用，Simplicity First。

## 7. 处理卡住的状态

部署期间卡住的 running turn（turn_start curl 超时断开致 handler 取消、driver 仍 park，turn 永远 running）：

```sql
-- 清理所有 running turn（无 driver 在 drain 了）
UPDATE turns SET status='interrupted' WHERE status='running';
-- 同步清理 pending approval
UPDATE approval_tickets SET status='interrupted' WHERE status='pending';
```

重启容器后 driver pool 重新预热，无遗留状态。

## 8. 经验沉淀 / 预防

1. **布尔 env 开关一律值检查，禁用 `is_ok()`**：`env::var(X).is_ok()` 对 `=0`/`=false`/空串都返回 true，是布尔开关的反模式。统一用 `.map(|v| v=="1"||v.eq_ignore_ascii_case("true")).unwrap_or(false)`。建议加 lint 或 helper `fn env_flag(name) -> bool`。
2. **docker-compose `${VAR:-default}` 的陷阱**：该语法让 env 在容器里**永远存在**（默认值注入），所以即便 `.env` 不写该变量，容器内 `is_ok()` 仍 true。值检查修复后此陷阱消除，但设计时仍应意识到"compose 声明的 env 永远存在"。
3. **e2e 验证必须覆盖"开关关闭"路径**：M8/M9 只验证了 `SIMULATE=1` 零回归 + 真实模型直连，从未验证 `SIMULATE=0` 经 docker-compose 部署后真正关闭。新增"开关关闭后走真实路径"的 e2e 用例。
4. **巡检**：`docker exec nexus-control printenv NEXUS_SIMULATE_APPROVAL` + `docker logs nexus-control | grep "simulated approval"` 可快速判断部署是否误入 SIMULATE。
5. **告警建议**：turn 落库 `model='nexus-gateway-mock'` 在非 SIMULATE 部署下应触发告警（说明真实模型未生效）。
