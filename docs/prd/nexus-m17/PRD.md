# PRD — Nexus M17 Skills 市场

## 背景
roadmap T12-3：企业 Skill 发布/版本/回滚。skills/skill_versions 表已建（initial.sql），无 handler。

## 目标
Skill 发布版本快照（version+checksum+content_ref）、激活版本（active_version_id）、回滚到历史版本、删除约束（有版本→409）。

## 非目标
- 不做 Skill 运行时执行（留 L5 Harness）
- 不碰 turn_start/drain/runtime

## 验收
| AC | 验收点 |
|---|---|
| AC1 | POST create（draft）+ GET list 本租户 |
| AC2 | POST version 发布（status→published，active_version_id 设置）|
| AC3 | GET versions 列出历史 |
| AC4 | POST rollback 激活历史版本 |
| AC5 | DELETE 有 versions→409 / 无→200 |
| AC6 | 零回归 |
