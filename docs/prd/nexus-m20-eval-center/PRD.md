# Nexus M20 PRD — 评测中心：版本化 Golden Set + 运行冻结快照

## 背景

Nexus 已交付 M12 评测中心骨架（`eval_cases` + `eval_runs` + status/contains 断言 + CI 门禁脚本），但仅是"一次性断言"：
- 用例无版本管理，无法追溯"某次评测用的是哪一版 Golden Set"；
- 评分仅 `status==expected && contains`，临床场景需按评分点（must_hit_points / must_not）逐点判定；
- 无运行冻结——运行中途改 rubric 不影响本次运行的能力缺失，不可复现不可对比。

本需求来自《AI 临研 Agent 平台评测系统设计方案》，核心思想：**所有实体一律版本化、不可变，运行时绑定到具体版本集合（快照），从根上保证可复现、可追溯**。

设计文档规划三阶段：
- **P1（MVP）**：Golden Set CRUD + 版本化实体 + 运行冻结快照 + rubric 点评分 ← **本里程碑**
- **P2（M21）**：Judge LLM + 质量门显著性检验 + CI 自动触发 + 失败样本回流
- **P3（M22）**：事件溯源 + as-of 回放 + Lineage 可视化 + 电子签名合规审计

## 目标

将评测从"一次性断言"升级为"有版本、有快照、可复现、可对比的受控资产"。

1. **版本化实体骨架** — 六类一等版本化实体（GoldenSet / Rubric / AgentRelease / JudgeConfig / EvalPipeline / Policy）统一遵循"草稿→审批→锁定（不可变）"模式，M20 激活 GoldenSet + Rubric 两类，其余建骨架表留后续激活
2. **Golden Set 管理** — Case 集合 + 语义化版本（patch/minor/major + 可比性标记）+ draft→approved→locked 生命周期 + 临床标注元数据（domain/difficulty/source/must_hit_points/must_not/acceptable_answers）
3. **Rubric 评分标准** — 评分点（criterion）+ 权重 + 容差，版本化，绑定到 GoldenSet 版本
4. **运行冻结快照** — EvalRun 启动第一步冻结 Manifest（golden_set 版本 hash + rubric 版本 hash + agent 引用 + policy 版本 hash），整个运行只读该 Manifest
5. **Rubric 驱动评分** — 升级 M12 断言为 rubric 逐点评分（确定性规则层：must_hit_points 命中率 + must_not 违规率 + acceptable_answers 匹配），Judge LLM 留 M21
6. **报告与对比** — 运行结果归档（逐 case 得分 + 聚合）+ 与基线 run 对比（同 golden_set 版本才可比）
7. **Web 控制台** — Golden Set 管理（版本列表/锁定/查看 case）+ 评测运行页（起 run / 查看逐 case 得分 / 基线对比）

## 范围（MVP / P1）

### 数据模型
- `entity_version` — 六类版本化实体共用骨架表（id/entity_type/entity_id/version_no/semver/manifest_hash/status/parent_id）
- `golden_sets` — Golden Set 逻辑实体（tenant_id/name/status/active_version_id）
- `golden_set_versions` — 版本快照（version_no/semver/manifest_hash/content_ref/status/distribution_meta）
- `golden_set_cases` — Case 全文（case_id_key/domain/difficulty/source/input_json/expected_json/phi_check_status）
- `rubrics` — Rubric 逻辑实体（tenant_id/name/status/active_version_id）
- `rubric_versions` — 版本快照（version_no/semver/content_ref/status）
- `eval_runs` — 升级现有表（加 snapshot_manifest JSONB + golden_set_version_id + rubric_version_id + agent_ref + trigger_type + baseline_run_id + gate_result）
- `eval_case_results` — 逐 case 结果（run_id/case_id/scores JSONB/trace_ref）升级现有 eval_runs 行级

### API
- GoldenSet：`POST/GET /v1/golden-sets`；`POST /v1/golden-sets/{id}/cases`；`GET /v1/golden-sets/{id}/cases`；`POST /v1/golden-sets/{id}/publish`（draft→approved→locked + 版本号）
- Rubric：`POST/GET /v1/rubrics`；`POST /v1/rubrics/{id}/publish`
- 评测运行：`POST /v1/evals/runs`（body: golden_set_id + rubric_id + agent thread_id 或 input；冻结快照→逐 case 评分→归档）；`GET /v1/evals/runs`；`GET /v1/evals/runs/{id}`（含逐 case 得分 + manifest）；`POST /v1/evals/runs/{id}/compare`（与基线对比）

### 评分（M20 仅确定性规则层）
- `must_hit_points` 命中率：检查 agent 输出是否包含每个必须命中的评分点（子串/正则匹配）
- `must_not` 违规检查：agent 输出命中任一禁止项→该 case 不通过
- `acceptable_answers` 匹配：输出在可接受答案集合内
- 结构合规：output 是否符合预期 schema（JSON 校验，可选）
- 聚合：维度分数 + 总分 + 逐 case 明细

### 运行冻结
- EvalRun 启动时构造 Manifest JSON：`{golden_set: {id, version, manifest_hash}, rubric: {id, version, manifest_hash}, agent: {thread_id/ref}, policy: {version}}`
- Manifest hash 存入 eval_runs.snapshot_hash
- 运行期间引用的 golden_set/rubric 版本不可变（已 locked）

## 非目标（留后续里程碑）

- **Judge LLM 评分**（M21/P2）：rubric 驱动的 LLM 逐点评分 + 3-Judge 投票 + judge 模型异源
- **CI 自动触发 + 质量门显著性检验**（M21/P2）：McNemar 检验、P0 阻断/P1 警告分级、AgentRelease 变更自动触发
- **失败样本回流**（M21/P2）：生产 trace → 脱敏 → 新 golden case → 防退化闭环
- **事件溯源 + as-of 回放**（M22/P3）：管理数据 append-only 事件流 + 时间旅行查询
- **Lineage 血缘可视化**（M22/P3）：报告→run→snapshot→golden_set→case→标注 全链下钻
- **电子签名合规**（M22/P3）：21 CFR Part 11 三要素
- **CAS/WORM 对象存储**（M22/P3）：内容寻址存储 + 对象锁，M20 用 PG + content_hash 字段
- **Temporal/Kafka/Neo4j**（speculative）：M20 不引入重基础设施，用 PG + 现有基础设施
- **AgentRelease/JudgeConfig/EvalPipeline/Policy 版本化激活**：M20 建骨架表但只激活 GoldenSet + Rubric

## 语义化版本策略

| 版本变更 | 定义 | 与旧版可比性 |
|---|---|---|
| `2.3.0 → 2.3.1` (patch) | 勘误/标注修订，case 语义不变 | ✅ 直接对比 |
| `2.3.1 → 2.4.0` (minor) | 仅新增 case / 标签 | ✅ 按 case-id 交集对比 |
| `2.4.0 → 3.0.0` (major) | 修改/删除已有 case、改 rubric | ⚠️ 标记不可直接对比，报告中显式提示 |

## 验收标准

- **AC20.1** 版本化实体骨架：`entity_version` 表建立（六类 entity_type 枚举，status draft/approved/locked）
- **AC20.2** GoldenSet CRUD：POST 创建（draft）+ GET 列表（tenant 隔离）+ GET 详情（含 case 列表）
- **AC20.3** GoldenSet Case 管理：POST 添加 case（含 input/expected/must_hit_points/must_not/acceptable_answers/domain/difficulty/source）+ GET case 列表
- **AC20.4** GoldenSet 发布版本：POST publish → draft→approved→locked + 自动 semver（首次 1.0.0，后续 patch+0.0.1 / minor+0.1.0 / major+1.0.0 由 body 指定 bump_type）+ 生成 manifest_hash + 版本不可变（locked 后不可再改）
- **AC20.5** Rubric CRUD + 发布版本：同 GoldenSet 模式（criterion 列表 + 权重 + 容差）
- **AC20.6** 评测运行冻结：POST /v1/evals/runs → 构造 Manifest（golden_set 版本 + rubric 版本 hash）→ 存 snapshot_hash → 逐 case 评分
- **AC20.7** Rubric 点评分：must_hit_points 命中率 + must_not 违规率 + acceptable_answers 匹配 → scores JSONB + 聚合
- **AC20.8** 运行报告：GET /v1/evals/runs/{id} 返回 manifest + 逐 case 得分 + 聚合维度
- **AC20.9** 基线对比：POST compare → 同 golden_set 版本才可比（不同版本标记不可直接对比）→ 输出 case 级 diff（pass↔fail 变化）
- **AC20.10** 零回归：M12 eval（eval_cases/eval_runs 旧表保留兼容）、M14 fork、M10 audit、M17 skills 不退化
- **AC20.11** Web 控制台：Golden Set 管理页 + 评测运行页（起 run / 查看得分 / 基线对比）

## 约束

- 不改 codex 内核（全部 nexus-control crate）
- 不删旧表（eval_cases/eval_runs 旧表 ALTER 升级，保留 M12 兼容；新表 golden_sets/golden_set_versions 等纯增量）
- 不引入 Temporal/Kafka/Neo4j（用 PG + 现有基础设施，Simplicity First）
- 评分仅确定性规则层（Judge LLM 留 M21，避免 speculative）
- 版本锁定后不可变（locked 状态的 version 行拒绝 UPDATE，应用层 + DB CHECK 双重）

## 测试用例

| # | 用例 | 验证 |
|---|---|---|
| T1 | 创建 GoldenSet + 添加 3 个 case | AC20.2/20.3 |
| T2 | 发布 GoldenSet v1.0.0 → 再加 case → 发布 v1.1.0 (minor) | AC20.4 semver + 不可变 |
| T3 | locked 版本尝试修改 → 拒绝 | AC20.4 不可变 |
| T4 | 创建 Rubric + 发布版本 | AC20.5 |
| T5 | 起评测 run（冻结快照）→ snapshot_hash 非空 + manifest 含版本 | AC20.6 |
| T6 | 逐 case 评分（must_hit 命中 + must_not 违规 + acceptable 匹配）→ scores 正确 | AC20.7 |
| T7 | GET run 报告 → manifest + 逐 case + 聚合 | AC20.8 |
| T8 | 起基线 run + 对比 run → case 级 diff（同版本可比） | AC20.9 |
| T9 | 不同 golden_set 版本对比 → 标记不可直接对比 | AC20.9 |
| T10 | 零回归：M12 eval/M14 fork/M10 audit/M17 skills | AC20.10 |
| T11 | Web：Golden Set 管理页 + 评测运行页 | AC20.11 |
