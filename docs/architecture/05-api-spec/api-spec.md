# Nexus 全部系统 API 清单（完整方案）

> 维度 05 · API 架构规格
> 架构层级对齐：L1 接入 / L2 网关 / L3 控制面 / L4 执行面 / L5 Harness / L6 模型 / L7 存储

## 目录

- [1. 概览](#1-概览)
- [2. A. Codex app-server JSON-RPC API（L5 Harness 集成面，复用）](#2-a-codex-app-server-json-rpc-apil5-harness-集成面复用)
  - [2.1 协议与传输](#21-协议与传输)
  - [2.2 生命周期 Lifecycle](#22-生命周期-lifecycle)
  - [2.3 Thread（会话）API](#23-thread会话-api)
  - [2.4 Thread Sections / Queue / Goal](#24-thread-sections--queue--goal)
  - [2.5 Background Terminals / Realtime](#25-background-terminals--realtime)
  - [2.6 Turn（对话轮次）API](#26-turn对话轮次-api)
  - [2.7 事件 ServerNotification（服务器推送通知）](#27-事件-servernotification服务器推送通知)
  - [2.8 审批 Server→Client 请求（HITL）](#28-审批-serverclient-请求hitl)
  - [2.9 工具/文件系统/MCP/插件/Skills/Hooks](#29-工具文件系统mcp插件skillshooks)
  - [2.10 配置/模型/账户](#210-配置模型账户)
  - [2.11 错误码](#211-错误码)
- [3. B. Nexus 控制平面 REST API（L2 网关，自建）](#3-b-nexus-控制平面-rest-apil2-网关自建)
  - [3.1 认证 Auth](#31-认证-auth)
  - [3.2 租户/组织/用户/RBAC](#32-租户组织用户rbac)
  - [3.3 工作区 Workspaces](#33-工作区-workspaces)
  - [3.4 会话 Threads（REST 网关 → 映射 app-server JSON-RPC）](#34-会话-threadsrest-网关--映射-app-server-json-rpc)
  - [3.5 审批中心 Approvals（HITL）](#35-审批中心-approvalshitl)
  - [3.6 连接器 Connectors / MCP Gateway](#36-连接器-connectors--mcp-gateway)
  - [3.7 计量/审计/成本](#37-计量审计成本)
  - [3.8 知识库 Knowledge Base / RAG](#38-知识库-knowledge-base--rag)
- [4. C. Webhook 与 WebSocket](#4-c-webhook-与-websocket)
  - [4.1 任务完成 Webhook](#41-任务完成-webhook)
  - [4.2 Webhook 管理](#42-webhook-管理)
  - [4.3 WebSocket 事件推送](#43-websocket-事件推送)
  - [4.4 SSE 降级](#44-sse-降级)
- [5. 鉴权与幂等设计](#5-鉴权与幂等设计)
- [6. 速率限制与配额](#6-速率限制与配额)

---

## 1. 概览

Nexus 平台 API 体系由三大类构成：

| 类别 | 层级 | 协议 | 鉴权 | 来源 | 方法数 |
|------|------|------|------|------|--------|
| A. Codex app-server JSON-RPC | L5 Harness | JSON-RPC 2.0 | 连接级 | 复用 Codex 黑盒 | ~180 |
| B. Nexus 控制平面 REST | L2 网关 | HTTPS REST | Bearer JWT + RBAC | 自建 | ~85 |
| C. Webhook / WebSocket | L1/L2 | HTTP 回调 / WS | HMAC / JWT | 自建 | 16 |

**设计原则**：
- L2 网关 REST API 是所有外部客户端的统一入口，内部通过 **JSON-RPC 桥接** 调用 L5 app-server
- app-server JSON-RPC 是 Harness 内核面，由 Nexus 执行面 Pod 直接消费，**不做外部暴露**
- Webhook 用 HMAC-SHA256 签名保证真实性，WebSocket 用权限驱动订阅控制数据范围
- 所有 REST 写操作支持 **幂等键**（`Idempotency-Key` header），防止网络重试导致重复操作

---

## 2. A. Codex app-server JSON-RPC API（L5 Harness 集成面，复用）

> 源码：`~/Nexus/codex-rs/app-server/`（crate `codex-app-server`）
> 文档：`~/Nexus/codex-rs/app-server/README.md`
> 传输：stdio / websocket / unix socket
> 角色：Nexus 执行面 Pod 内部调用，**不对外暴露**

### 2.1 协议与传输

| 项 | 说明 |
|----|------|
| 协议 | JSON-RPC 2.0（省略 `"jsonrpc":"2.0"` 头） |
| stdio | `--stdio` 或 `--listen stdio://`，newline-delimited JSON |
| websocket | `--listen ws://IP:PORT`（**实验性/不稳定**） |
| unix socket | `--listen unix://PATH`，websocket over unix socket |
| 健康检查 | `GET /readyz`（200 once listener ready）, `GET /healthz`（200 无 Origin） |
| 背压 | 饱和时返回 error code `-32001` "Server overloaded; retry later." |
| 通知抑制 | `initialize.params.capabilities.optOutNotificationMethods`（精确匹配） |

### 2.2 生命周期 Lifecycle

| 方法 | 方向 | 参数 | 响应 | 说明 |
|------|------|------|------|------|
| `initialize` | Client→Server | `clientInfo{name,title,version}`, `capabilities{experimentalApi, optOutNotificationMethods, extensions{...}}` | `userAgent`, `codexHome`, `platformFamily`, `platformOs` | 每连接一次，先于任何其他方法。声明 MCP 扩展（`openai/form`, `openai/elicitation`, `io.modelcontextprotocol/ui`） |
| `initialized` | Client→Server | 无 | 无（通知） | `initialize` 响应后发送，完成握手 |

### 2.3 Thread（会话）API

| 方法 | 参数 | 响应 | 说明 |
|------|------|------|------|
| `thread/start` | `model?`, `cwd?`, `sandbox?`/`permissions?`, `approvalPolicy?`, `personality?`, `serviceName?`, `sessionStartSource?`, `dynamicTools?`, `projectId?`, `historyMode?`, `ephemeral?`, `environments?`, `runtimeWorkspaceRoots?`, `selectedCapabilityRoots?` | `{thread{id,preview,modelProvider,createdAt,ephemeral,path}}` | 创建新会话。自动订阅 turn/item 事件。`ephemeral:true` 内存临时会话。`permissions` 优先于 `sandbox`。发出 `thread/started` 通知 |
| `thread/resume` | `threadId`, `excludeTurns?`, `initialTurnsPage?`, `cwd?`, 配置覆盖 | `{thread{...}, turnsBackwardsCursor?, itemsBackwardsCursor?, initialTurnsPage?}` | 恢复已存会话。`excludeTurns:true` 跳过全量历史（分页模式推荐）。冷恢复加载配置不持有全局元数据锁 |
| `thread/fork` | `threadId`, `lastTurnId?`, `beforeTurnId?`, `ephemeral?`, `excludeTurns?`, `deferGoalContinuation?`, 配置覆盖 | `{thread{id,sessionId,forkedFromId}}` | 从已有会话分叉。`lastTurnId` 包含到该 turn，`beforeTurnId` 不包含。发出 `thread/started` |
| `thread/archive` | `threadId` | `{}` | 移动 rollout 到归档目录。发出 `thread/archived` |
| `thread/unarchive` | `threadId` | `{thread{...}}` | 恢复归档会话。发出 `thread/unarchived` |
| `thread/delete` | `threadId` | `{}` | 硬删除会话及后代。发出 `thread/deleted` |
| `thread/read` | `threadId`, `includeTurns?` | `{thread{id,status,turns[]}}` | 只读读取会话，不加载。`includeTurns:true` 加载历史（分页模式已废弃，用 `thread/turns/list`） |
| `thread/list` | `cursor?`, `limit?`, `sortKey?`, `sortDirection?`, `modelProviders?`, `sourceKinds?`, `originators?`, `archived?`, `sectionId?`, `cwd?`, `useStateDbOnly?`, `searchTerm?`, `projectId?`, `parentThreadId?`, `ancestorThreadId?` | `{data[],nextCursor,backwardsCursor}` | 分页列举会话。支持多维度过滤 |
| `thread/search` | `searchTerm`, `cursor?`, `limit?` | `{data[],nextCursor}` | 搜索会话标题 |
| `thread/searchOccurrences` | `threadId`, `searchTerm`, `limit?` | `{data[{turnId,itemId,snippet,snippetMatchRange,turnCursor}],nextCursor}` | 实验性：在单个分页会话内搜索文本 |
| `thread/loaded/list` | 无 | `{data[threadId]}` | 列出当前内存中加载的会话 |
| `thread/turns/list` | `threadId`, `limit?`, `sortDirection?`, `itemsView?`, `cursor?` | `{data[turn],nextCursor,backwardsCursor}` | 分页列举会话 turn 历史 |
| `thread/items/list` | `threadId`, `turnId?`, `limit?`, `sortDirection?`, `cursor?` | `{data[{turnId,item}],nextCursor}` | 分页列举会话 item |
| `thread/inject_items` | `threadId`, `items[]` | `{}` | 注入原始 Responses API items 到历史，不启动 turn |
| `thread/timeline/list` | `threadId`, `cursor?`, `limit?` | `{data[],nextCursor,activeRealtimeSessionAtPageStart}` | 实验性：分页列举 timeline（turn items + realtime facts + turn 边界） |
| `thread/unsubscribe` | `threadId` | `{status}` | 取消订阅。最后一个订阅者退出后 60s 卸载会话，发出 `thread/closed` |
| `thread/rollback` | `threadId`, `count` | `{thread{...,turns[]}}` | **已废弃**。从上下文移除最后 N 个 turn |
| `thread/revert` | `threadId`, `beforeTurnId` | `{thread{...}}` | 恢复分页会话到指定 turn 前缀。中断活动 turn，发出 `thread/reverted` |
| `thread/compact/start` | `threadId` | `{}` | 触发历史压缩。进度通过 turn/item 通知推送 |
| `thread/shellCommand` | `threadId`, `command`, `timeoutMs?` | `{}` | 运行用户 `!` 命令（unsandboxed，full access） |
| `thread/approveGuardianDeniedAction` | `threadId`, `itemId` | `{}` | 手动批准先前被 Guardian 拒绝的操作 |
| `thread/metadata/update` | `threadId`, `gitInfo?`, `projectId?`, `daybreakEnabled?` | `{thread{...}}` | 更新会话元数据 |
| `thread/name/set` | `threadId`, `name` | `{}` | 设置会话名称。发出 `thread/name/updated` |
| `thread/memoryMode/set` | `threadId`, `mode("enabled"\|"disabled")` | `{}` | 实验性：设置会话内存资格 |
| `memory/reset` | 无 | `{}` | 实验性：清除 CODEX_HOME/memories 和 sqlite 内存数据 |

### 2.4 Thread Sections / Queue / Goal

| 方法 | 参数 | 响应 | 说明 |
|------|------|------|------|
| `threadSection/list` | `cursor?`, `limit?` | `{data[section]}` | 分页列举独立持久化的会话分区 |
| `threadSection/create` | `displayName`, `appearance?` | `{section{...}}` | 创建自定义分区（UUID） |
| `threadSection/update` | `sectionId`, `displayName?`, `appearance?` | `{section{...}}` | 重命名/更新分区外观 |
| `threadSection/delete` | `sectionId` | `{}` | 删除自定义分区，成员 thread 原子性移到未分区列表 |
| `thread/section/move` | `threadId`, `sectionId`, `beforeThreadId?` | `{}` | 原子性移动 thread 到分区 |
| `thread/settings/update` | `threadId`, `model?`, `effort?`, `summary?`, `sandboxPolicy?`/`permissions?`, `serviceTier?`, `developer_instructions?` | `{}` | 实验性：排队更新下次 turn 设置。发出 `thread/settings/updated`（如果实际变更） |
| `thread/queue/add` | `threadId`, `input[]`, `clientUserMessageId` | `{queuedSubmission{id,input,clientUserMessageId}}` | 实验性：持久化排队用户 turn，FIFO 自动提交 |
| `thread/queue/list` | `threadId`, `cursor?`, `limit?` | `{data[],nextCursor}` | 实验性：列出排队 turn |
| `thread/queue/update` | `threadId`, `queuedSubmissionId`, `input[]` | `{}` | 实验性：编辑排队 turn |
| `thread/queue/delete` | `threadId`, `queuedSubmissionId` | `{}` | 实验性：删除排队 turn |
| `thread/queue/reorder` | `threadId`, `queuedSubmissionIds[]` | `{}` | 实验性：替换排队 turn 顺序 |
| `thread/queue/start` | `threadId`, `queuedSubmissionId?` | `{turn{...}}` | 实验性：启动队列头或指定排队 turn |
| `thread/goal/set` | `threadId`, `objective?`, `tokenBudget?`, `status?` | `{goal{...}}` | 创建/更新会话目标。发出 `thread/goal/updated` |
| `thread/goal/get` | `threadId` | `{goal?}` | 读取当前目标 |
| `thread/goal/clear` | `threadId` | `{cleared:bool}` | 清除当前目标。发出 `thread/goal/cleared` |

### 2.5 Background Terminals / Realtime

| 方法 | 参数 | 响应 | 说明 |
|------|------|------|------|
| `thread/backgroundTerminals/clean` | `threadId` | `{}` | 实验性：终止会话所有后台终端 |
| `thread/backgroundTerminals/list` | `threadId`, `cursor?`, `limit?` | `{data[{itemId,processId,command,cwd,...}],nextCursor}` | 实验性：列出运行中后台终端 |
| `thread/backgroundTerminals/terminate` | `threadId`, `processId` | `{terminated:bool}` | 实验性：终止指定后台终端 |
| `thread/realtime/start` | `threadId`, `outputModality`, `model?`, `version?`, `prompt?`, `transport?`, `includeStartupContext?`, `initialItems?`, `realtimeStartInstructions?`, `realtimeEndInstructions?`, `codexResponseHandoffMode?`, `delegationAckFiller?`, `clientManagedHandoffs?`, `codexResponsesAsItems?` | `{}` | 实验性：启动会话实时会话。V1/V2/V3 协议版本 |
| `thread/realtime/appendAudio` | `threadId`, `audio` | `{}` | 实验性：追加输入音频块 |
| `thread/realtime/appendText` | `threadId`, `text`, `role` | `{}` | 实验性：追加文本输入 |
| `thread/realtime/appendSpeech` | `threadId`, `text` | `{}` | 实验性：追加模型应朗读的文本 |
| `thread/realtime/stop` | `threadId` | `{}` | 实验性：停止实时会话 |
| `thread/realtime/listVoices` | 无 | `{voices[]}` | 实验性：列出可用语音 |

### 2.6 Turn（对话轮次）API

| 方法 | 参数 | 响应 | 说明 |
|------|------|------|------|
| `turn/start` | `threadId`, `input[]`, `clientUserMessageId?`, `toolOutput?`, `model?`, `effort?`, `summary?`, `cwd?`, `sandboxPolicy?`/`permissions?`, `approvalPolicy?`, `approvalsReviewer?`, `personality?`, `outputSchema?`, `serviceTier?`, `serviceTierForTurn?`, `environments?`, `runtimeWorkspaceRoots?`, `turnTrigger?`, `cyberAccessProgram?` | `{turn{id,status,items[],error}}` | 发送用户输入并启动 Codex 生成。发出 `turn/started`、`item/*`、`turn/completed` |
| `turn/settings/update` | `threadId`, `turnId`, `model?`, `effort?`, `summary?`, `serviceTier?`, `approvalsReviewer?` | `{status:"applied"\|"targetUnavailable"}` | 实验性：发布设置补丁到活跃 turn。模型切换需 `step_model_switching` |
| `turn/steer` | `threadId`, `input[]`, `clientUserMessageId?`, `expectedTurnId` | `{turnId}` | 向活动 turn 追加用户输入（不启动新 turn） |
| `turn/interrupt` | `threadId`, `turnId` | `{}` | 取消活动 turn。发出 `turn/completed`（status: interrupted） |

### 2.7 事件 ServerNotification（服务器推送通知）

> 事件是 JSON-RPC notification（无 `id` 字段），服务器主动推送。

#### 2.7.1 Thread 生命周期事件

| 事件 | 参数 | 说明 |
|------|------|------|
| `thread/started` | `{thread{...}}` | 会话启动/分叉/取消归档时 |
| `thread/status/changed` | `{threadId, status}` | 会话状态变化。status: `notLoaded`/`idle`/`active`/`systemError` |
| `thread/archived` | `{threadId}` | 会话归档 |
| `thread/unarchived` | `{threadId}` | 会话取消归档 |
| `thread/closed` | `{threadId}` | 会话卸载关闭 |
| `thread/deleted` | `{threadId}` | 会话删除 |
| `thread/name/updated` | `{threadId, name}` | 会话名称更新 |
| `thread/settings/updated` | `{threadId, threadSettings}` | 会话设置变更（实验性） |
| `thread/tokenUsage/updated` | `{threadId, tokenUsage}` | token 用量更新 |
| `thread/goal/updated` | `{threadId, goal}` | 目标变更 |
| `thread/goal/cleared` | `{threadId}` | 目标清除 |
| `thread/queue/changed` | `{threadId}` | 排队 turn 变化（实验性） |
| `thread/reverted` | `{threadId}` | 会话恢复 |
| `thread/environment/connected` | `{threadId, environmentId}` | 环境连接成功（实验性） |
| `thread/environment/disconnected` | `{threadId, environmentId}` | 环境断开（实验性） |
| `thread/project/updated` | `{...}` | 项目分配变更（实验性） |
| `project/changed` | `{...}` | 项目提交变更（实验性） |

#### 2.7.2 Turn 事件

| 事件 | 参数 | 说明 |
|------|------|------|
| `turn/started` | `{turn{id,status,items[],error}}` | turn 开始运行 |
| `turn/completed` | `{turn{...}}` | turn 完成。status: `completed`/`interrupted`/`failed` |
| `turn/diff/updated` | `{threadId, turnId, diff}` | turn 级统一 diff 快照 |
| `turn/plan/updated` | `{turnId, explanation?, plan[]}` | agent 分享/变更计划 |
| `turn/moderationMetadata` | `{threadId, turnId, metadata}` | 审核元数据（实验性） |
| `rawResponse/completed` | `{threadId, turnId, responseId, usage}` | 原始 Responses API 完成（实验性，需 `experimentalRawEvents`） |
| `model/safetyBuffering/updated` | `{threadId, turnId, model, useCases, reasons, showBufferingUi, fasterModel}` | 安全缓冲中 |
| `model/rerouted` | `{threadId, turnId, fromModel, toModel, reason}` | 模型重新路由 |
| `model/verification` | `{threadId, turnId, verifications}` | 账户验证标记 |
| `modelProvider/authRecoveryStarted` | `{threadId, turnId, provider, message}` | 认证恢复开始 |
| `modelProvider/authRecoveryCompleted` | `{threadId, turnId, provider, message}` | 认证恢复完成 |

#### 2.7.3 Item 事件

| 事件 | 参数 | 说明 |
|------|------|------|
| `item/started` | `{item{...}}` | 新工作单元开始。item 类型：`userMessage`/`agentMessage`/`plan`/`reasoning`/`commandExecution`/`fileChange`/`mcpToolCall`/`collabToolCall`/`subAgentActivity`/`webSearch`/`imageGeneration`/`imageView`/`sleep`/`enteredReviewMode`/`exitedReviewMode`/`contextCompaction`/`functionCallOutput`/`dynamicToolCall` |
| `item/completed` | `{item{...}}` | 工作单元完成（权威结果） |
| `item/agentMessage/delta` | `{itemId, delta}` | agent 消息流式文本追加 |
| `item/plan/delta` | `{itemId, delta}` | plan 内容流式（实验性） |
| `item/reasoning/summaryTextDelta` | `{itemId, summaryIndex, delta}` | 推理摘要流式 |
| `item/reasoning/summaryPartAdded` | `{itemId, summaryIndex}` | 推理摘要分段边界 |
| `item/reasoning/textDelta` | `{itemId, contentIndex, delta}` | 原始推理文本流式 |
| `item/commandExecution/outputDelta` | `{itemId, stream, deltaBase64?, aggregatedOutput?}` | 命令输出流式 |
| `item/fileChange/patchUpdated` | `{itemId, ...}` | 文件变更补丁流式快照 |
| `item/fileChange/outputDelta` | `{itemId, ...}` | **已废弃**。旧版 apply_patch 输出 |
| `item/autoApprovalReview/started` | `{threadId, turnId, targetItemId, review, action}` | [不稳定] 自动审批审查开始 |
| `item/autoApprovalReview/completed` | `{threadId, turnId, targetItemId, review, action}` | [不稳定] 自动审批审查完成 |
| `autoApprovalReview/strictReviewRequired` | `{threadId, turnId, startedAtMs}` | 实验性：需同步审批审查 |
| `compacted` | `{threadId, turnId}` | **已废弃**。用 `contextCompaction` item 替代 |

#### 2.7.4 Realtime 事件（实验性）

| 事件 | 参数 | 说明 |
|------|------|------|
| `thread/realtime/started` | `{threadId, realtimeSessionId}` | 实时会话开始 |
| `thread/realtime/itemAdded` | `{threadId, item}` | 非音频实时 item |
| `thread/realtime/transcript/delta` | `{threadId, role, delta}` | 实时转录增量 |
| `thread/realtime/transcript/done` | `{threadId, role, text}` | 实时转录完成 |
| `thread/realtime/item/started` | `{threadId, item}` | 实时 item 开始 |
| `thread/realtime/item/transcript/delta` | `{threadId, itemId, delta}` | 转录段增量 |
| `thread/realtime/item/completed` | `{threadId, item}` | 实时 item 完成 |
| `thread/realtime/outputAudio/delta` | `{threadId, audio}` | 输出音频块 |
| `thread/realtime/error` | `{threadId, message}` | 实时错误 |
| `thread/realtime/closed` | `{threadId, reason}` | 实时传输关闭 |
| `thread/realtime/sdp` | `{threadId, sdp}` | WebRTC SDP 应答 |

#### 2.7.5 其他通知

| 事件 | 参数 | 说明 |
|------|------|------|
| `error` | `{threadId?, error{message, codexErrorInfo?, additionalDetails?, misalignment?}}` | 运行时错误 |
| `warning` | `{threadId?, message}` | 非致命警告 |
| `configWarning` | `{summary, details?, path?, range?}` | 配置/初始化警告 |
| `skills/changed` | `{}` | 技能文件变更 |
| `app/list/updated` | `{data[]}` | 应用列表更新 |
| `fuzzyFileSearch/sessionUpdated` | `{sessionId, query, files}` | 模糊搜索更新（实验性） |
| `fuzzyFileSearch/sessionCompleted` | `{sessionId, query}` | 模糊搜索完成（实验性） |
| `mcpServer/startupStatus/updated` | `{threadId, name, status, error, failureReason}` | MCP 服务器启动状态 |
| `mcpServer/event/stream/notification` | `{subscriptionId, notification}` | MCP 事件流通知（实验性） |
| `windowsSandbox/setupCompleted` | `{mode, success, error}` | Windows 沙箱设置完成 |
| `serverRequest/resolved` | `{threadId, requestId}` | 服务端请求已解决/清理 |
| `account/login/completed` | `{loginId, success, error}` | 登录完成 |
| `account/updated` | `{authMode, planType}` | 认证模式变更 |
| `account/rateLimits/updated` | `{...}` | 速率限制变更（稀疏滚动更新） |
| `mcpServer/oauthLogin/completed` | `{name, threadId, success, error?}` | OAuth 登录完成 |
| `remoteControl/status/changed` | `{status, serverName, environmentId}` | 远程控制状态变更（实验性） |

### 2.8 审批 Server→Client 请求（HITL）

> 这些是 Server 主动发送给 Client 的 JSON-RPC **request**（带 `id`），Client 必须响应。

| 方法 | 方向 | 参数 | Client 响应 | 说明 |
|------|------|------|------------|------|
| `item/commandExecution/requestApproval` | Server→Client | `{threadId, turnId, itemId, environmentId, kind("command"\|"writeStdin"), approvalId?, command?, cwd?, commandActions?, reason, additionalPermissions?, proposedExecpolicyAmendment?, proposedNetworkPolicyAmendments?, availableDecisions?}` | `{decision: "accept"\|"acceptForSession"\|{acceptWithExecpolicyAmendment:{...}}\|{applyNetworkPolicyAmendment:{...}}\|"decline"\|"cancel"}` | 命令执行审批。`kind` 区分新命令 vs stdin 写入 |
| `item/fileChange/requestApproval` | Server→Client | `{threadId, turnId, itemId, reason?, grantRoot?}` | `{decision: "accept"\|"acceptForSession"\|"decline"\|"cancel"}` | 文件变更审批 |
| `item/permissions/requestApproval` | Server→Client | `{threadId, turnId, itemId, environmentId, cwd, reason, permissions{fileSystem{write[]}}}` | `{scope?, permissions{fileSystem{write[]}}}` | 权限请求。响应中只授予的子集生效，未列出=拒绝 |
| `item/tool/requestUserInput` | Server→Client | `{threadId, turnId, itemId, isBlocking, questions[]}` | `{answers[]}` | 工具请求用户输入（1-3 个问题） |
| `mcpServer/elicitation/request` | Server→Client | `{threadId, turnId?, serverName, mode("form"\|"openaiForm"\|"openai/form"\|"url"), message, requestedSchema?/url?/elicitationId?}` | `{action: "accept", content}\|{action:"decline"\|"cancel", content:null}` | MCP 服务器结构化输入请求 |
| `item/tool/call` | Server→Client | `{threadId, turnId, callId, namespace, tool, arguments}` | `{contentItems[], success}` | 动态工具调用（实验性，需 `experimentalApi`） |
| `attestation/generate` | Server→Client | `{threadId}` | `{token: "v1.<opaque>"}` | 客户端证明生成。用于 `x-oai-attestation` |
| `currentTime/read` | Server→Client | `{threadId}` | `{currentTimeAt: <unix_seconds>}` | 实验性：外部时钟源读取 |

### 2.9 工具/文件系统/MCP/插件/Skills/Hooks

| 方法 | 参数 | 响应 | 说明 |
|------|------|------|------|
| `app/installed` | `threadId?`, `forceRefresh?` | `{apps[{id,runtimeName,enabled,callable}]}` | 读取已安装应用状态 |
| `app/list` | `cursor?`, `limit?`, `threadId?`, `forceRefetch?` | `{data[],nextCursor}` | 列出可用应用 |
| `app/read` | `appIds[]`, `threadId?`, `includeTools?` | `{apps[], missingAppIds[]}` | 批量读取应用元数据（最多 100 个） |
| `fs/readFile` | `path` | `{dataBase64}` | 读取绝对路径文件 |
| `fs/writeFile` | `path`, `dataBase64` | `{}` | 写入绝对路径文件 |
| `fs/createDirectory` | `path`, `recursive?` | `{}` | 创建目录（recursive 默认 true） |
| `fs/getMetadata` | `path` | `{isDirectory, isFile, isSymlink, createdAtMs, modifiedAtMs}` | 获取路径元数据 |
| `fs/readDirectory` | `path` | `{data[{fileName, isDirectory, isFile}]}` | 列出直接子条目 |
| `fs/remove` | `path`, `recursive?`, `force?` | `{}` | 删除文件/目录树（recursive/force 默认 true） |
| `fs/copy` | `sourcePath`, `destPath`, `recursive?` | `{}` | 复制文件/目录 |
| `fs/watch` | `watchId`, `path` | `{path}` | 订阅文件系统变更通知 |
| `fs/unwatch` | `watchId` | `{}` | 停止文件系统变更通知 |
| `fs/changed` | — | `{watchId, changedPaths[]}` | 文件变更通知 |
| `plugin/list` | `cwds?`, `forceRefetch?`, `kinds?` | `{data[], marketplaceLoadErrors[]}` | 列出插件市场与状态 |
| `plugin/search` | `searchTerm`, `scope?`, `cwds?`, `cursor?`, `limit?` | `{data[],nextCursor}` | 搜索插件 |
| `plugin/installed` | `cwds?`, `suggestionNames?` | `{data[]}` | 列出已安装插件 |
| `plugin/reconcile` | 无 | `{changedPlugins{hasMcps,hasApps,hasHooks,hasSkills}}` | 同步远程插件状态 |
| `plugin/read` | `marketplacePath`, `pluginName` | `{...summary, manifest}` | 读取插件详情 |
| `plugin/skill/read` | `remoteMarketplaceName`, `remotePluginId`, `skillName` | `{markdown}` | 读取远程插件技能 markdown |
| `plugin/install` | `marketplacePath`, `pluginName`, `installAttemptId?` | `{authPolicy, appsNeedingAuth[]}` | 安装插件 |
| `plugin/uninstall` | `pluginId` | `{}` | 卸载插件 |
| `skills/list` | `cwds?`, `forceReload?` | `{data[{cwd, skills[], errors[]}]}` | 列出技能 |
| `skills/extraRoots/set` | `extraRoots[]` | `{}` | 替换运行时额外技能根 |
| `skills/config/write` | `path?`, `name?`, `enabled` | `{}` | 写入技能配置 |
| `hooks/list` | `cwds[]` | `{data[{cwd, hooks[], warnings[], errors[]}]}` | 列出发现的 hooks |
| `marketplace/add` | `source` | `{rootPath, alreadyPresent}` | 添加远程插件市场 |
| `marketplace/remove` | `marketplaceName` | `{}` | 移除插件市场 |
| `marketplace/upgrade` | `marketplaceName?` | `{names[], upgradedRoots[], errors[]}` | 升级插件市场 |
| `mcpServer/oauth/login` | `server`, `threadId?`, `clientRegistration?` | `{authorization_url}` | MCP 服务器 OAuth 登录 |
| `mcpServer/tool/call` | `threadId`, `server`, `tool`, `arguments?`, `_meta?` | `{result}` | 调用 MCP 工具 |
| `mcpServer/resource/read` | `threadId?`, `server`, `uri`, `originCallId?`, `connectorId?` | `{contents}` | 读取 MCP 资源 |
| `mcpServerStatus/list` | `threadId?`, `detail?`, `cursor?`, `limit?` | `{data[],nextCursor}` | 列出 MCP 服务器状态 |
| `config/mcpServer/reload` | 无 | `{}` | 重载 MCP 配置 |
| `mcpServer/event/stream/start` | `threadId`, `server`, `subscriptionId`, `name`, `arguments`, `_meta?` | — | 实验性：订阅 MCP 事件 |
| `mcpServer/event/stream/stop` | `subscriptionId` | — | 实验性：停止 MCP 事件订阅 |
| `command/exec` | `command[]`, `processId?`, `cwd?`, `env?`, `size?`, `permissionProfile?`/`sandboxPolicy?`, `outputBytesCap?`, `disableOutputCap?`, `timeoutMs?`, `disableTimeout?`, `tty?`, `streamStdin?`, `streamStdoutStderr?` | `{exitCode, stdout, stderr}` | 沙箱内执行命令 |
| `command/exec/write` | `processId`, `deltaBase64?`, `closeStdin?` | `{}` | 写入命令 stdin |
| `command/exec/resize` | `processId`, `size` | `{}` | 调整 PTY 大小 |
| `command/exec/terminate` | `processId` | `{}` | 终止命令 |
| `command/exec/outputDelta` | — | `{processId, stream, deltaBase64, capReached}` | 命令输出增量通知 |
| `process/spawn` | `command[]`, `processHandle`, `cwd`, `env?`, `outputBytesCap?`, `timeoutMs?`, `tty?`, `size?`, `streamStdoutStderr?` | `{}` | 实验性：非沙箱进程启动 |
| `process/writeStdin` | `processHandle`, `deltaBase64?`, `closeStdin?` | `{}` | 实验性：写进程 stdin |
| `process/resizePty` | `processHandle`, `size` | `{}` | 实验性：调整 PTY |
| `process/kill` | `processHandle` | `{}` | 实验性：终止进程 |
| `process/outputDelta` | — | `{processHandle, stream, deltaBase64, capReached}` | 实验性：进程输出增量通知 |
| `process/exited` | — | `{processHandle, exitCode, stdout, stdoutCapReached, stderr, stderrCapReached}` | 实验性：进程退出通知 |
| `fuzzyFileSearch/sessionStart` | — | — | 实验性：模糊文件搜索会话 |
| `fuzzyFileSearch/query` | — | — | 实验性：模糊文件搜索查询 |

### 2.10 配置/模型/账户

| 方法 | 参数 | 响应 | 说明 |
|------|------|------|------|
| `config/read` | 无 | `{...config}` | 读取运行时有效配置 |
| `config/value/write` | `keyPath`, `value` | `{}` | 写入单个配置键。受管键拒绝 |
| `config/batchWrite` | `edits[{keyPath, value, mergeStrategy}]`, `reloadUserConfig?` | `{}` | 原子批量写入配置 |
| `configRequirements/read` | 无 | `{...requirements}` | 读取受管要求约束 |
| `model/list` | `includeHidden?` | `{data[{id, reasoningEfforts, modelSpecialty?, ...}]}` | 列出可用模型 |
| `modelProvider/capabilities/read` | 无 | `{...capabilities}` | 读取模型供应商能力 |
| `permissionProfile/list` | `cwd?`, `cursor?`, `limit?` | `{data[],nextCursor}` | beta：列出权限配置 |
| `experimentalFeature/list` | `threadId?`, `cursor?`, `limit?` | `{data[],nextCursor}` | 列出功能标志 |
| `experimentalFeature/enablement/set` | `features{key: value}` | `{}` | 设置运行时功能开关 |
| `collaborationMode/list` | 无 | `{data[]}` | 列出协作模式预设（实验性） |
| `environment/add` | `environmentId`, `execServerUrl`, `connectTimeoutMs?` | `{}` | 添加远程环境（实验性） |
| `environment/info` | `environmentId` | `{shell, cwd}` | 环境信息（实验性） |
| `environment/status` | `environmentId` | `{status}` | 环境状态（实验性） |
| `account/read` | `refreshToken?` | `{account{type,email?,planType?}, requiresOpenaiAuth}` | 读取当前账户 |
| `account/login/start` | `type`, apiKey?/— | `{type}` | 开始登录。type: `apiKey`/`chatgpt`/`chatgptDeviceCode`/`amazonBedrock`/`amazonBedrockAccessKeys` |
| `account/login/cancel` | `loginId` | `{}` | 取消待处理登录 |
| `account/logout` | 无 | `{}` | 登出 |
| `account/rateLimits/read` | 无 | `{...rateLimits}` | 读取速率限制 |
| `account/usage/read` | `threadId?` | `{...usage}` | 读取用量。`threadId` 时返回单会话用量 |
| `account/workspaceMessages/read` | 无 | `{messages[]}` | 工作区消息 |
| `account/rateLimitResetCredit/consume` | `idempotencyKey`, `resetCreditId?` | `{...}` | 消费速率限制重置额度 |
| `account/sendAddCreditsNudgeEmail` | 无 | `{}` | 发送添加额度提醒邮件 |
| `account/bedrock/discover` | — | — | 实验性：发现 AWS 配置 |
| `account/bedrock/setup` | — | — | 实验性：设置 Bedrock |
| `review/start` | `threadId`, `delivery?`, `target{type,...}` | `{turn{...}, reviewThreadId}` | 启动代码审查。target: `uncommittedChanges`/`baseBranch`/`commit`/`custom` |
| `server/diagnostics` | 无 | `{process{id,...}, gauges[]}` | 实验性：服务器诊断（内存/仪表） |
| `feedback/upload` | `classification`, `reason?`, `conversation_id`, `extraLogFiles?` | `{threadId}` | 上传反馈报告 |
| `remoteControl/enable` | `ephemeral?` | `{status snapshot}` | 实验性：启用远程控制 |
| `remoteControl/disable` | `ephemeral?` | `{status snapshot}` | 实验性：禁用远程控制 |
| `remoteControl/status/read` | 无 | `{status, serverName, environmentId}` | 实验性：远程控制状态 |
| `remoteControl/pairing/start` | `manualCode?` | `{pairingCode, manualPairingCode?, environmentId, expiresAt}` | 实验性：配对开始 |
| `remoteControl/pairing/status` | `pairingCode?`/`manualPairingCode?` | `{claimed}` | 实验性：配对状态轮询 |
| `remoteControl/client/list` | `environmentId`, `cursor?`, `limit?`, `order?` | `{data[],nextCursor}` | 实验性：列出控制器设备 |
| `remoteControl/client/revoke` | `environmentId`, `clientId` | `{}` | 实验性：撤销控制器 |
| `windowsSandbox/setupStart` | `mode`, `cwd?` | `{started:true}` | Windows 沙箱设置 |
| `externalAgentConfig/detect` | `includeHome?`, `cwds?`, `migrationSource?` | `{...items, connectorCandidates[]}` | 实验性：检测外部 Agent 配置 |
| `externalAgentConfig/import` | `migrationItems[]`, `migrationSource?`, `source?`, `providerId?` | `{importId}` | 实验性：导入外部 Agent 配置 |
| `externalAgentConfig/import/readHistories` | 无 | `{...}` | 实验性：读取导入历史 |
| `project/list` | `sortKey?`, `sortDirection?`, `limit?`, `cursor?` | `{data[],nextCursor}` | 实验性：列出项目 |
| `project/read` | `projectId` | `{...}` | 实验性：读取项目 |
| `project/create` | `idempotencyKey`, `roots[]`, `metadata?` | `{...}` | 实验性：创建项目 |
| `project/import` | `idempotencyKey`, `roots[]`, `metadata?`, `threadIds?` | `{...}` | 实验性：导入项目 |
| `project/update` | `projectId`, `metadata?` | `{...}` | 实验性：更新项目 |
| `project/move` | `projectId`, `beforeProjectId?` | `{...}` | 实验性：移动项目 |
| `project/delete` | `projectId` | `{...}` | 实验性：删除项目（不清除 thread/文件） |

### 2.11 错误码

| 错误码 | 说明 |
|--------|------|
| `-32001` | 服务器过载，稍后重试 |
| `-32600` | 请求无效（如 parent-owned Multi-Agent V2 子 agent 拒绝直接输入） |
| `-32601` | 方法不存在（如 `thread/items/list` 不支持） |

`codexErrorInfo` 枚举值：

| 值 | 说明 |
|----|------|
| `ContextWindowExceeded` | 上下文窗口超限 |
| `SessionBudgetExceeded` | 会话预算超限 |
| `UsageLimitExceeded` | 用量限制 |
| `rateLimitExceeded` | 上游速率限制 |
| `misalignmentPolicyViolation` | 对齐策略违规（非重试） |
| `HttpConnectionFailed` | HTTP 连接失败 |
| `ResponseStreamConnectionFailed` | 响应流连接失败 |
| `ResponseStreamDisconnected` | 响应流中断 |
| `ResponseTooManyFailedAttempts` | 重试次数耗尽 |
| `ActiveTurnNotSteerable` | 活动 turn 不可 steer |
| `BadRequest` | 请求格式错误 |
| `Unauthorized` | 未授权 |
| `SandboxError` | 沙箱错误 |
| `InternalServerError` | 内部错误 |
| `Other` | 未分类 |

---

## 3. B. Nexus 控制平面 REST API（L2 网关，自建）

> 层级：L2 网关层
> 协议：HTTPS REST，JSON 请求/响应
> 鉴权：`Authorization: Bearer <JWT>` + RBAC 角色
> 幂等：所有 POST/PUT/DELETE 支持 `Idempotency-Key` header
> 分页：`?cursor=<opaque>&limit=<N>`，响应 `{data, nextCursor, backwardsCursor}`

### 3.1 认证 Auth

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| POST | `/auth/login` | `{email, password, mfaCode?}` | `{accessToken, refreshToken, expiresIn}` | 无 | 用户名密码登录。支持 MFA |
| POST | `/auth/refresh` | `{refreshToken}` | `{accessToken, expiresIn}` | refreshToken | 刷新访问令牌 |
| POST | `/auth/logout` | — | `{}` | Bearer | 登出，撤销当前会话 |
| GET | `/auth/me` | — | `{userId, tenantId, orgUnitId, roles[], permissions[]}` | Bearer | 当前用户信息与权限 |
| POST | `/auth/api-keys` | `{name, scopes[], expiresIn?}` | `{apiKeyId, apiKey(一次可见)}` | Bearer | 创建 API Key |
| DELETE | `/auth/api-keys/{id}` | — | `{}` | Bearer | 撤销 API Key |
| GET | `/auth/sessions` | — | `{data[{sessionId, device, ip, lastActiveAt}]}` | Bearer | 列出活跃会话 |
| DELETE | `/auth/sessions/{id}` | — | `{}` | Bearer | 撤销指定会话 |

### 3.2 租户/组织/用户/RBAC

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| GET | `/tenants` | `cursor?, limit?` | `{data[], nextCursor}` | Bearer + `tenant:read` | 列出租户（仅平台管理员） |
| POST | `/tenants` | `{name, plan, maxSeats?, maxConcurrency?}` | `{tenant{...}}` | Bearer + `tenant:create` | 创建租户 |
| GET | `/tenants/{id}` | — | `{tenant{...}}` | Bearer + `tenant:read` | 读取租户 |
| PUT | `/tenants/{id}` | `{name?, plan?, maxSeats?}` | `{tenant{...}}` | Bearer + `tenant:update` | 更新租户 |
| GET | `/org-units` | `tenantId?, cursor?, limit?` | `{data[], nextCursor}` | Bearer + `org:read` | 列出组织单元 |
| POST | `/org-units` | `{name, parentId?, tenantId}` | `{orgUnit{...}}` | Bearer + `org:create` | 创建组织单元 |
| PUT | `/org-units/{id}` | `{name?}` | `{orgUnit{...}}` | Bearer + `org:update` | 更新组织单元 |
| DELETE | `/org-units/{id}` | — | `{}` | Bearer + `org:delete` | 删除组织单元 |
| GET | `/users` | `orgUnitId?, cursor?, limit?, search?` | `{data[], nextCursor}` | Bearer + `user:read` | 列出用户 |
| POST | `/users` | `{email, name, orgUnitId, roles[]}` | `{user{...}}` | Bearer + `user:create` | 创建用户。发送邀请邮件 |
| GET | `/users/{id}` | — | `{user{...}}` | Bearer + `user:read` | 读取用户 |
| PUT | `/users/{id}` | `{name?, orgUnitId?, roles?}` | `{user{...}}` | Bearer + `user:update` | 更新用户 |
| DELETE | `/users/{id}` | — | `{}` | Bearer + `user:delete` | 删除用户（软删除） |
| GET | `/roles` | — | `{data[{id, name, permissions[]}]}` | Bearer + `role:read` | 列出角色 |
| POST | `/roles` | `{name, permissions[]}` | `{role{...}}` | Bearer + `role:create` | 创建角色 |
| PUT | `/roles/{id}` | `{name?, permissions?}` | `{role{...}}` | Bearer + `role:update` | 更新角色 |
| DELETE | `/roles/{id}` | — | `{}` | Bearer + `role:delete` | 删除角色 |
| GET | `/users/{id}/memberships` | — | `{data[{orgUnitId, role, since}]}` | Bearer + `membership:read` | 列出用户成员关系 |
| POST | `/users/{id}/memberships` | `{orgUnitId, role}` | `{membership{...}}` | Bearer + `membership:create` | 添加成员关系 |
| DELETE | `/memberships/{id}` | — | `{}` | Bearer + `membership:delete` | 移除成员关系 |

### 3.3 工作区 Workspaces

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| GET | `/workspaces` | `cursor?, limit?, orgUnitId?` | `{data[], nextCursor}` | Bearer + `workspace:read` | 列出工作区 |
| POST | `/workspaces` | `{name, orgUnitId?, settings?}` | `{workspace{...}}` | Bearer + `workspace:create` | 创建工作区 |
| GET | `/workspaces/{id}` | — | `{workspace{...}}` | Bearer + `workspace:read` | 读取工作区 |
| PUT | `/workspaces/{id}` | `{name?, settings?}` | `{workspace{...}}` | Bearer + `workspace:update` | 更新工作区 |
| DELETE | `/workspaces/{id}` | — | `{}` | Bearer + `workspace:delete` | 删除工作区 |
| GET | `/workspaces/{id}/members` | `cursor?, limit?` | `{data[], nextCursor}` | Bearer + `workspace:read` | 列出工作区成员 |
| POST | `/workspaces/{id}/members` | `{userId, role}` | `{membership{...}}` | Bearer + `workspace:update` | 添加工作区成员 |
| PUT | `/workspaces/{id}/members/{uid}` | `{role?}` | `{membership{...}}` | Bearer + `workspace:update` | 更新成员角色 |
| DELETE | `/workspaces/{id}/members/{uid}` | — | `{}` | Bearer + `workspace:update` | 移除工作区成员 |
| GET | `/workspaces/{id}/settings` | — | `{settings{...}}` | Bearer + `workspace:read` | 读取工作区设置 |
| PUT | `/workspaces/{id}/settings` | `{settings{...}}` | `{settings{...}}` | Bearer + `workspace:update` | 更新工作区设置 |

### 3.4 会话 Threads（REST 网关 → 映射 app-server JSON-RPC）

> Nexus REST 网关接收 HTTP 请求，内部通过 JSON-RPC 桥接调用 L5 app-server。
> 下表标注 `→` 表示映射到的 app-server JSON-RPC 方法。

| Method | Path | 参数 | 响应 | 鉴权 | JSON-RPC 映射 | 说明 |
|--------|------|------|------|------|--------------|------|
| GET | `/threads` | `cursor?, limit?, sortKey?, archived?, workspaceId?` | `{data[], nextCursor, backwardsCursor}` | Bearer + `thread:read` | `thread/list` | 分页列举会话 |
| POST | `/threads` | `{input[], model?, cwd?, workspaceId?, sandbox?, approvalPolicy?, idempotencyKey}` | `{thread{id,...}, turn{...}}` | Bearer + `thread:create` | `thread/start` + `turn/start` | 创建会话并启动首轮。响应中返回 thread 和 turn 对象 |
| GET | `/threads/{id}` | `includeTurns?` | `{thread{...}}` | Bearer + `thread:read` | `thread/read` | 读取会话 |
| DELETE | `/threads/{id}` | — | `{}` | Bearer + `thread:delete` | `thread/delete` | 删除会话 |
| POST | `/threads/{id}/resume` | `{cwd?, model?}` | `{thread{...}}` | Bearer + `thread:update` | `thread/resume` | 恢复会话 |
| POST | `/threads/{id}/fork` | `{lastTurnId?, ephemeral?}` | `{thread{...}}` | Bearer + `thread:create` | `thread/fork` | 分叉会话 |
| POST | `/threads/{id}/archive` | — | `{}` | Bearer + `thread:update` | `thread/archive` | 归档会话 |
| GET | `/threads/{id}/turns` | `cursor?, limit?, sortDirection?, itemsView?` | `{data[], nextCursor, backwardsCursor}` | Bearer + `thread:read` | `thread/turns/list` | 分页列举 turn |
| POST | `/threads/{id}/turns` | `{input[], model?, effort?, approvalPolicy?, idempotencyKey}` | `{turn{id, status, items[], error}}` | Bearer + `thread:update` | `turn/start` | 发送用户输入启动新 turn |
| POST | `/threads/{id}/interrupt` | `{turnId}` | `{}` | Bearer + `thread:update` | `turn/interrupt` | 中断活动 turn |
| POST | `/threads/{id}/steer` | `{input[], expectedTurnId?}` | `{turnId}` | Bearer + `thread:update` | `turn/steer` | 向活动 turn 追加输入 |
| GET | `/threads/{id}/items` | `turnId?, cursor?, limit?, sortDirection?` | `{data[], nextCursor}` | Bearer + `thread:read` | `thread/items/list` | 分页列举 item |
| POST | `/threads/{id}/messages` | `{text}` | `{message{...}}` | Bearer + `thread:update` | `turn/start`（包装为 `input[{type:"text", text}]`） | 快捷发送文本消息 |
| GET | `/threads/{id}/timeline` | `cursor?, limit?` | `{data[], nextCursor}` | Bearer + `thread:read` | `thread/timeline/list` | 分页时间线 |
| POST | `/threads/{id}/compact` | — | `{}` | Bearer + `thread:update` | `thread/compact/start` | 触发历史压缩 |
| GET | `/threads/{id}/search` | `searchTerm, limit?` | `{data[], nextCursor}` | Bearer + `thread:read` | `thread/searchOccurrences` | 会话内搜索 |
| PUT | `/threads/{id}/metadata` | `{gitInfo?, projectId?}` | `{thread{...}}` | Bearer + `thread:update` | `thread/metadata/update` | 更新元数据 |
| PUT | `/threads/{id}/settings` | `{model?, effort?, sandbox?}` | `{}` | Bearer + `thread:update` | `thread/settings/update` | 更新会话设置 |
| POST | `/threads/{id}/shell` | `{command, timeoutMs?}` | `{}` | Bearer + `thread:update` | `thread/shellCommand` | 运行 shell 命令 |

### 3.5 审批中心 Approvals（HITL）

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| GET | `/approvals` | `threadId?, status?, cursor?, limit?` | `{data[], nextCursor}` | Bearer + `approval:read` | 列出审批 |
| GET | `/approvals/pending` | `workspaceId?, cursor?, limit?` | `{data[], nextCursor}` | Bearer + `approval:read` | 列出待处理审批 |
| POST | `/approvals` | `{threadId, turnId, itemId, decision, scope?}` | `{approval{...}}` | Bearer + `approval:create` | 提交审批决策。decision: `accept`/`acceptForSession`/`decline`/`cancel`。映射 JSON-RPC `item/commandExecution/requestApproval` 响应或 `item/fileChange/requestApproval` 响应 |
| GET | `/approvals/{id}` | — | `{approval{...}}` | Bearer + `approval:read` | 读取审批详情 |
| POST | `/approvals/{id}/decide` | `{decision, scope?}` | `{approval{...}}` | Bearer + `approval:update` | 对待处理审批做出决策 |
| GET | `/approvals/stats` | `workspaceId?, from?, to?` | `{total, pending, approved, declined, avgResponseMs}` | Bearer + `approval:read` | 审批统计 |
| GET | `/approvals/rules` | `workspaceId?` | `{data[{rule}]}` | Bearer + `approval:read` | 审批规则列表 |
| PUT | `/approvals/rules` | `{rules[]}` | `{rules[]}` | Bearer + `approval:update` | 更新审批规则 |

### 3.6 连接器 Connectors / MCP Gateway

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| GET | `/connectors` | `workspaceId?, cursor?, limit?` | `{data[], nextCursor}` | Bearer + `connector:read` | 列出连接器 |
| POST | `/connectors` | `{name, type, config{...}, workspaceId?}` | `{connector{...}}` | Bearer + `connector:create` | 注册连接器 |
| GET | `/connectors/{id}` | — | `{connector{...}}` | Bearer + `connector:read` | 读取连接器 |
| PUT | `/connectors/{id}` | `{name?, config?, enabled?}` | `{connector{...}}` | Bearer + `connector:update` | 更新连接器 |
| DELETE | `/connectors/{id}` | — | `{}` | Bearer + `connector:delete` | 删除连接器 |
| GET | `/connectors/{id}/tools` | — | `{data[{name, description, inputSchema, readOnlyHint}]}` | Bearer + `connector:read` | 列出连接器工具 |
| POST | `/connectors/{id}/tools/call` | `{tool, arguments}` | `{result}` | Bearer + `connector:call` | 调用工具。映射 `mcpServer/tool/call` |
| GET | `/connectors/{id}/health` | — | `{status, latencyMs, lastCheckedAt}` | Bearer + `connector:read` | 连接器健康检查 |
| GET | `/connectors/{id}/resources` | `cursor?, limit?` | `{data[], nextCursor}` | Bearer + `connector:read` | 列出 MCP 资源 |
| POST | `/connectors/{id}/oauth/start` | `{redirectUri}` | `{authorizationUrl}` | Bearer + `connector:update` | 启动 OAuth 授权 |
| POST | `/connectors/{id}/oauth/callback` | `{code, state}` | `{success}` | Bearer + `connector:update` | OAuth 回调 |

### 3.7 计量/审计/成本

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| GET | `/usage` | `tenantId?, workspaceId?, userId?, threadId?, model?, from?, to?, groupBy?` | `{data[], summary{totalTokens, totalCost, totalRequests}}` | Bearer + `usage:read` | 多维归因用量查询。支持按 tenant/workspace/user/thread/model 维度聚合 |
| GET | `/usage/breakdown` | `tenantId?, from?, to?, dimension?` | `{data[{label, tokens, cost, percentage}]}` | Bearer + `usage:read` | 用量维度分解 |
| GET | `/usage/export` | `tenantId?, from?, to?, format?(csv/json)` | `{downloadUrl}` | Bearer + `usage:read` | 导出用量报表 |
| GET | `/usage/realtime` | `workspaceId?` | `{activeThreads, activeTurns, tokensPerMinute, requestsPerMinute}` | Bearer + `usage:read` | 实时用量监控 |
| GET | `/audit-logs` | `tenantId?, userId?, action?, resourceType?, from?, to?, cursor?, limit?` | `{data[], nextCursor}` | Bearer + `audit:read` | 审计日志查询（WORM 存储） |
| GET | `/audit-logs/{id}` | — | `{auditLog{...}}` | Bearer + `audit:read` | 审计日志详情 |
| POST | `/audit-logs/export` | `{tenantId?, from?, to?, format?}` | `{downloadUrl}` | Bearer + `audit:read` | 导出审计日志 |
| GET | `/cost-dashboard` | `tenantId?, workspaceId?, from?, to?` | `{totalCost, byModel[], byWorkspace[], byUser[], trend[]}` | Bearer + `cost:read` | 成本仪表盘 |
| GET | `/cost-dashboard/by-tenant` | `from?, to?` | `{data[{tenantId, tenantName, cost, tokens}]}` | Bearer + `cost:read` | 按租户成本分解（平台管理员） |
| GET | `/cost-dashboard/by-workspace` | `tenantId?, from?, to?` | `{data[{workspaceId, workspaceName, cost, tokens}]}` | Bearer + `cost:read` | 按工作区成本分解 |
| GET | `/cost-dashboard/by-model` | `tenantId?, from?, to?` | `{data[{model, cost, tokens, requests}]}` | Bearer + `cost:read` | 按模型成本分解 |

### 3.8 知识库 Knowledge Base / RAG

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| GET | `/kb/documents` | `collectionId?, cursor?, limit?, search?` | `{data[], nextCursor}` | Bearer + `kb:read` | 列出文档 |
| POST | `/kb/documents` | `{title, content?, source?, collectionId?, metadata?}` | `{document{...}}` | Bearer + `kb:write` | 上传文档。支持文件上传 |
| GET | `/kb/documents/{id}` | — | `{document{...}}` | Bearer + `kb:read` | 读取文档 |
| PUT | `/kb/documents/{id}` | `{title?, content?, metadata?}` | `{document{...}}` | Bearer + `kb:write` | 更新文档 |
| DELETE | `/kb/documents/{id}` | — | `{}` | Bearer + `kb:write` | 删除文档 |
| POST | `/kb/search` | `{query, collectionId?, topK?, filters?}` | `{results[{documentId, title, snippet, score, collectionId}]}` | Bearer + `kb:read` | 语义搜索（ACL 过滤：只返回用户有权访问的文档） |
| POST | `/kb/embeddings` | `{texts[]}` | `{embeddings[]}` | Bearer + `kb:write` | 生成嵌入向量 |
| GET | `/kb/collections` | `cursor?, limit?` | `{data[], nextCursor}` | Bearer + `kb:read` | 列出文档集合 |
| POST | `/kb/collections` | `{name, description?, acl?}` | `{collection{...}}` | Bearer + `kb:write` | 创建文档集合 |
| DELETE | `/kb/collections/{id}` | — | `{}` | Bearer + `kb:write` | 删除文档集合 |
| POST | `/kb/ingest` | `{source{type, ...}, collectionId?}` | `{jobId, status}` | Bearer + `kb:write` | 批量摄取（URL/crawler/upload） |
| GET | `/kb/stats` | `collectionId?` | `{totalDocuments, totalChunks, totalTokens, indexSize}` | Bearer + `kb:read` | 知识库统计 |

---

## 4. C. Webhook 与 WebSocket

### 4.1 任务完成 Webhook

> 用户注册 Webhook URL，Nexus 在事件发生时向该 URL 发送 HTTP POST 请求。
> 每个请求携带 HMAC-SHA256 签名头，接收方需验证签名。

**请求格式**：

```
POST {callback_url}
Content-Type: application/json
X-Nexus-Signature: hmac_sha256(request_body, webhook_secret)
X-Nexus-Event: {event_type}
X-Nexus-Delivery: {delivery_id}
X-Nexus-Timestamp: {unix_timestamp}

{
  "event": "turn.completed",
  "tenantId": "...",
  "workspaceId": "...",
  "threadId": "...",
  "turnId": "...",
  "status": "completed",
  "tokenUsage": {...},
  "timestamp": "2026-09-06T00:00:00Z",
  "deliveryId": "..."
}
```

**支持的事件类型**：

| 事件 | 触发条件 | 载荷 |
|------|---------|------|
| `turn.completed` | Turn 完成（completed/interrupted/failed） | `{threadId, turnId, status, tokenUsage, items[]}` |
| `approval.requested` | 审批请求产生 | `{threadId, turnId, itemId, kind, command?}` |
| `thread.archived` | 会话归档 | `{threadId}` |
| `goal.blocked` | 目标状态变为 blocked | `{threadId, goal{...}}` |
| `usage.threshold_exceeded` | 用量达到阈值（80%/90%/100%） | `{tenantId, workspaceId?, currentUsage, threshold, limit}` |

**幂等性**：每个 delivery 有唯一 `deliveryId`，接收方应去重。Nexus 在 5xx 响应时自动重试（指数退避，最多 5 次）。

### 4.2 Webhook 管理

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| POST | `/webhooks` | `{url, events[], secret, workspaceId?, isActive?}` | `{webhook{...}}` | Bearer + `webhook:create` | 注册 Webhook |
| GET | `/webhooks` | `workspaceId?, cursor?, limit?` | `{data[], nextCursor}` | Bearer + `webhook:read` | 列出 Webhook |
| PUT | `/webhooks/{id}` | `{url?, events?, isActive?}` | `{webhook{...}}` | Bearer + `webhook:update` | 更新 Webhook |
| DELETE | `/webhooks/{id}` | — | `{}` | Bearer + `webhook:delete` | 删除 Webhook |
| POST | `/webhooks/{id}/test` | — | `{deliveryId, responseCode}` | Bearer + `webhook:update` | 测试 Webhook |
| GET | `/webhooks/{id}/deliveries` | `cursor?, limit?, status?` | `{data[], nextCursor}` | Bearer + `webhook:read` | 列出投递记录 |
| POST | `/webhooks/{id}/deliveries/{did}/retry` | — | `{deliveryId}` | Bearer + `webhook:update` | 重试投递 |

### 4.3 WebSocket 事件推送

> 权限驱动订阅：客户端通过 WebSocket 连接后，只能收到其有权访问的会话/工作区的事件。

**连接**：

```
WS /ws/threads/{threadId}/events
Authorization: Bearer <JWT>
```

**订阅事件**（连接后自动订阅该 thread 的所有事件）：

| 通道 | 推送内容 | 说明 |
|------|---------|------|
| `thread/{threadId}/events` | thread/turn/item 事件实时推送 | 映射 app-server JSON-RPC notification。包括 `thread/started`、`thread/status/changed`、`turn/started`、`turn/completed`、`item/started`、`item/completed`、`item/agentMessage/delta` 等 |
| `item/agentMessage/delta` | 流式 agent 消息增量 | 低延迟文本流（WebSocket 文本帧） |
| `item/commandExecution/outputDelta` | 命令输出流 | base64 编码的输出块 |
| `turn/started` / `turn/completed` | Turn 生命周期 | 启动/完成通知 |
| `approval` | 审批请求推送 + 响应 | 审批请求推送，客户端可发送决策响应 |
| `dashboard` | 全局用量/状态/告警 | 实时仪表盘推送 |

**WebSocket 消息格式**（服务器→客户端）：

```json
{
  "type": "notification",
  "method": "item/agentMessage/delta",
  "params": {
    "threadId": "thr_123",
    "itemId": "item_456",
    "delta": "Hello, "
  }
}
```

**WebSocket 消息格式**（客户端→服务器，审批决策）：

```json
{
  "type": "response",
  "id": "req_789",
  "result": {
    "decision": "accept"
  }
}
```

### 4.4 SSE 降级

> 当 WebSocket 不可用时（企业代理/防火墙），提供 SSE 降级通道。

| Method | Path | 参数 | 响应 | 鉴权 | 说明 |
|--------|------|------|------|------|------|
| GET | `/threads/{id}/events/stream` | `Accept: text/event-stream` | `text/event-stream` | Bearer + `thread:read` | SSE 降级推送。事件格式：`event: item/agentMessage/delta\ndata: {...}\n\n` |
| GET | `/dashboard/stream` | `Accept: text/event-stream` | `text/event-stream` | Bearer + `usage:read` | 全局仪表盘 SSE 降级 |

---

## 5. 鉴权与幂等设计

### 5.1 鉴权层级

| 层级 | 机制 | 适用范围 |
|------|------|---------|
| 连接级 | app-server stdio/unix socket（无鉴权，Pod 内通信） | L5 Harness 内部 |
| 网关级 | Bearer JWT（短期 access token，15min 过期） | L2 REST API |
| 网关级 | Bearer JWT（长期 refresh token，7d 过期） | L2 刷新令牌 |
| API Key | `X-API-Key` header，绑定 scopes | 外部系统集成 |
| Webhook | HMAC-SHA256 签名验证 | 回调请求 |
| WebSocket | JWT 握手 + 权限驱动订阅 | 实时事件 |
| RBAC | 角色权限矩阵（`tenant:read`、`thread:create` 等） | 所有 REST 路由 |

### 5.2 RBAC 权限矩阵

| 角色 | 关键权限 |
|------|---------|
| `platform_admin` | `tenant:*`, `org:*`, `user:*`, `role:*`, `cost:read` |
| `tenant_admin` | `org:*`, `user:*`, `workspace:*`, `connector:*`, `usage:read`, `audit:read` |
| `workspace_owner` | `workspace:*`, `thread:*`, `connector:*`, `kb:*`, `webhook:*`, `approval:*` |
| `workspace_member` | `thread:read/create/update`, `kb:read`, `approval:read/update` |
| `workspace_viewer` | `thread:read`, `kb:read`, `usage:read` |
| `api_client` | 自定义 scopes（`thread:read`, `thread:create` 等） |

### 5.3 幂等设计

- **REST 写操作**：所有 POST/PUT/DELETE 支持 `Idempotency-Key` header。Nexus 在 24h 内去重相同 key 的请求，返回首次结果。
- **Webhook 投递**：每个 delivery 有唯一 `deliveryId`，接收方应去重。
- **JSON-RPC 桥接**：REST 请求的 `Idempotency-Key` 传递到 app-server 的 `turn/start` 等方法作为 `clientUserMessageId`。

---

## 6. 速率限制与配额

| 维度 | 限制 | 超限行为 |
|------|------|---------|
| 全局 QPS | 100 req/s（per tenant） | 429 + `Retry-After` |
| 并发会话 | 10 active threads（per workspace） | 429 + 排队提示 |
| 并发 Turn | 1 active turn（per thread） | 429 + "turn already active" |
| 模型调用 | 按 tenant+model 配额预扣 | 超限时 `UsageLimitExceeded` |
| Webhook 投递 | 5 次重试（指数退避 1s/2s/4s/8s/16s） | 超过 5 次标记为 failed |
| WebSocket 连接 | 50 concurrent WS（per user） | 新连接被拒绝 |
| API Key 调用 | 1000 req/h（per key） | 429 + `X-RateLimit-Reset` |

**配额预扣**：Turn 启动前，Nexus 根据模型和历史用量预扣 token 配额。Turn 完成后按实际用量结算差额。预扣不足时拒绝启动（`SessionBudgetExceeded`）。

---

*文件路径：`~/Nexus/docs/architecture/05-api-spec/api-spec.md`*
*关联产物：`api-overview.svg` / `api-overview.png` / `api-spec-report.html`*
*源码参考：`~/Nexus/codex-rs/app-server/README.md`*
