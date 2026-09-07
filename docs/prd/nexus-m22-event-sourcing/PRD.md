# Nexus M22 事件溯源 + Lineage + 电子签名 — PRD

## 背景
M20 评测中心 P3 阶段。M20 已交付版本化实体 + 确定性评分 + 运行冻结快照；M21 已交付 Judge LLM + CI 质量门。M22 补齐合规审计与可追溯闭环。

## 目标
1. **事件溯源**：核心领域实体状态变更落 append-only 事件流，支持 as-of 时间旅行查询（回放某时刻实体状态）
2. **Lineage 血缘**：评测报告 → batch_run → snapshot → golden_set → case → 标注 全链下钻查询 API
3. **电子签名**：21 CFR Part 11 三要素（签名者身份 + 签名时间 + 签名含义），对 locked 版本签名
4. **CAS/WORM 对象存储**：内容寻址存储（content_hash 主键 + WORM trigger），大对象原文落 CAS，content_ref 存 hash 引用

## 设计原则
- Simplicity First：不引入 Kafka/EventStore/Neo4j/外部 CAS，用 PG（append-only 表 + WORM trigger + content_hash）
- Surgical Changes：纯增量，不删 M20/M21 表，不改 start_run drain
- M10 已验证 WORM trigger 模式，复用

## AC 验收标准
- AC22.1 事件落库：entity 版本 publish / run 创建 / run 完成 / 审批决策 → domain_events 落 append-only 事件
- AC22.2 WORM：domain_events UPDATE/DELETE → trigger 拒绝
- AC22.3 as-of 回放：GET /v1/events/{entity_type}/{entity_id}?as_of=ISO8601 → 按序回放该时刻状态
- AC22.4 Lineage 查询：GET /v1/lineage/eval_batch_run/{id} → 上下游链（golden_set → cases → run → results → audit）
- AC22.5 电子签名：POST /v1/esign → 记录 signer + purpose + signature_hash + 21 CFR 三要素
- AC22.6 签名不可篡改：esign_records WORM，UPDATE/DELETE → 拒绝
- AC22.7 CAS 写入：POST /v1/content → content_hash 算 SHA-256 → content_store 存原文（去重）→ 返回 hash
- AC22.8 CAS WORM：content_store 不可改不可删
- AC22.9 CAS 引用：entity_version publish 时 content_ref 可存 CAS hash，读取经 CAS 解引用
- AC22.10 签名验证：GET /v1/esign/{id}/verify → 重算 hash 比对 + 三要素完整性校验
- AC22.11 零回归：M20/M21 评测 + M10 audit + M15 pool 不退化

## 非目标
- 3-Judge 投票（留后续）
- McNemar 显著性检验（留后续）
- 全量历史迁移（旧表不迁事件流，新动作起记）
- 对象存储 S3/MinIO 后端（PG BYTEA 够 MVP）
