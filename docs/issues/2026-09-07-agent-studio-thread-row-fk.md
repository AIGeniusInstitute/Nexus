# Issue: Agent Studio — ThreadRow 缺 agent_def_id + delete_agent FK 违约

## 1. 用户现象
- Studio 会话列表「Agent」列对所有会话都显示「默认」，即使该会话创建时绑定了 Agent。
- 在 Agent 定义管理中删除一个已绑定会话的 Agent，删除无反应（实际 500），Agent 仍在列表中。

## 2. 问题描述
- `GET /v1/threads` 响应只有 `id/title/status/created_at`，缺 `agent_def_id` 字段 → 前端无法显示绑定关系。
- `DELETE /v1/agents/{id}` 在该 Agent 有绑定 thread 时，`threads.agent_def_id` 外键违约（NO ACTION）→ DELETE 失败 → 500。

## 3. 根因
- `http_server.rs` 的 `ThreadRow` 结构体未声明 `agent_def_id` 字段，`threads_list` 的 SELECT 也未查该列。
- `agent_defs.rs delete_agent` 直接 `DELETE FROM agent_definitions`，未处理 `threads.agent_def_id` 外键引用；migration 的 FK 是 `REFERENCES agent_definitions(id)`（默认 ON DELETE NO ACTION）。

## 4. 复现路径
1. login → POST /v1/agents (创建) → POST /v1/threads {agent_def_id} → GET /v1/threads → 响应无 agent_def_id。
2. DELETE /v1/agents/{刚创建} → 500（FK 违约），GET /v1/agents 仍含该 Agent。

## 5. 诊断方法
```
curl ... GET /v1/threads | python3 -m json.tool   # 响应 keys 无 agent_def_id
docker exec nexus-pg-m4 psql -U nexus -d nexus -c "\d threads"  # FK agent_def_id NO ACTION
curl ... DELETE /v1/agents/{id}   # 500
```

## 6. 修复方案
- `ThreadRow` 加 `agent_def_id: Option<i64>` + SELECT 加该列（Surgical，2 行改动）。
- `delete_agent` 改事务：先 `UPDATE threads SET agent_def_id=NULL WHERE agent_def_id=$id` 再 DELETE（解绑，不破坏 thread；Simplicity First，app 层处理避免 schema FK 改动）。

## 7. 经验沉淀 / 预防
- 新增表字段后，list API 的 Row 结构体 + SELECT 必须同步加列（M1 ThreadRow 是 M1 时代结构，Agent Studio 加列未同步）。
- 有外键引用的实体删除，必须考虑引用方处理（SET NULL / cascade / 409 明确报错）。
