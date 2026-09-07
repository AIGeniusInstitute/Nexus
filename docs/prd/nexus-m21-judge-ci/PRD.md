# Nexus M21 评测中心 — Judge LLM + CI 质量门 + 失败样本回流

## 背景
M20 交付了版本化 GoldenSet + Rubric + 运行冻结快照 + **确定性规则层评分**（must_hit_points 命中率 + must_not 违规 + acceptable_answers 匹配）。但确定性评分无法判定主观维度（临床推理质量、逻辑连贯性、安全性语义），需 LLM Judge 叠加。同时评测结果需接入 CI 流水线作为发布门禁，失败样本需回流形成防退化闭环。

## 目标
1. **JudgeConfig 版本化** — 激活 M20 entity_version 骨架表的 judge_config 类型：judge prompt 模板 + 评分维度 + judge 模型配置，draft→publish(locked) 不可变
2. **Judge LLM 评分层** — 给定 agent_output + rubric criteria + judge config，调用 LLM 逐维度评分（0-1 + rationale），与 M20 确定性评分叠加
3. **CI 质量门** — eval-gate 脚本集成批量运行 + 质量阈值（P0 阻断/P1 警告）+ 显著性对比（accuracy delta vs baseline）
4. **失败样本回流** — 生产 turn 的 agent_output → 脱敏 → 新 golden case（仅 draft golden set），形成防退化闭环

## 范围（P2 MVP）
- 单 Judge（MVP；3-Judge 投票留扩展点，judge 模型异源留 config 字段）
- Judge 直接调 dashscope chat API（不经过 codex/gateway；Judge 是文本评分，无需 harness/工具调用）
- SIMULATE_JUDGE 模式（NEXUS_SIMULATE_JUDGE=1 返回合成评分，e2e 可验证 pipeline 无需真实模型）
- 显著性检验 MVP 用 accuracy delta + 阈值（McNemar 统计检验留扩展）

## 非目标（留后续）
- 3-Judge 多数投票 + judge 模型异源（需要多模型路由 + 配额）
- McNemar / Wilcoxon 统计检验库（需引入统计 crate）
- AgentRelease 变更自动触发评测（需 CI webhook 集成）
- 自动脱敏（PHI/PII 识别 + 替换；MVP 原样存 + source 标注）

## 验收标准
- **AC21.1** JudgeConfig 创建（draft）+ 发布版本（semver + manifest_hash + locked 不可变）
- **AC21.2** JudgeConfig locked 后不可改（add dimension 拒绝）
- **AC21.3** Judge LLM 评分函数：给定 output + rubric + judge config → 结构化 JSON（dimensions[].score + rationale + overall_score + passed）
- **AC21.4** SIMULATE_JUDGE 模式：NEXUS_SIMULATE_JUDGE=1 返回合成评分（不调真实模型，pipeline 可验证）
- **AC21.5** start_run 集成 Judge：run 关联 judge_config_id → scores 含 deterministic + judge 两层
- **AC21.6** 聚合含 avg_judge_score
- **AC21.7** CI 质量门脚本：跑批量 run → accuracy < P0 阈值 exit 1 / accuracy delta > P1 阈值 warn
- **AC21.8** 失败样本回流：POST /v1/golden-sets/{id}/cases/from-turn → 从 turn agent_output 创建新 case（仅 draft golden set）
- **AC21.9** 跨租户隔离（judge_configs / 回流 case 均 tenant-scoped）
- **AC21.10** 零回归（M20 版本化 GoldenSet + 确定性评分 + 基线对比不退化）

## 测试用例
- T1: 创建 JudgeConfig(draft) + 添加维度 → 发布 v1.0.0（manifest_hash + locked）
- T2: locked 后添加维度 → 400
- T3: Judge LLM 评分（SIMULATE_JUDGE=1）→ 合成结构化评分
- T4: Judge LLM 评分（真实 dashscope）→ 真实逐维度评分
- T5: start_run 关联 judge_config → scores 含 deterministic + judge
- T6: 聚合 avg_judge_score
- T7: CI eval-gate-v2.sh P0 阻断（accuracy < 阈值）exit 1
- T8: CI eval-gate-v2.sh P1 警告（accuracy delta）exit 0
- T9: 失败样本回流 → draft golden set 新增 case（source=trace）
- T10: locked golden set 回流 → 拒绝
- T11: 零回归（M20 publish/score/compare 不退化）

## 约束
- 不改 codex 内核（全部 nexus-control crate）
- 不删旧表（M20 的 6 表不动，纯增量）
- Simplicity First：不引入统计 crate、不引入多模型路由
- Surgical Changes：不重构 start_run drain 循环，Judge 评分在 score_case 之后叠加
