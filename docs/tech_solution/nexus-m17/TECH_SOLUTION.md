# 技术方案 — Nexus M17 Skills 市场

## 改动（纯增量）
- migration 20260906000012：ALTER skills ADD description/status(draft/published/archived)/owner_user_id/active_version_id/updated_at
- skills.rs（新）：SkillRow/SkillVersionRow + create/list/get/publish_version(事务 INSERT version+UPDATE active)/list_versions/rollback(UPDATE active)/delete(有 versions→409)
- http_server.rs：6 路由 + map_skill_err
- lib.rs + db.rs + main.rs 接线

## 关键决策
- publish_version 用事务（INSERT version + UPDATE active 原子）
- rollback 不删版本（只切 active_version_id，保留历史）
- 删除约束（有 versions→409 保留版本历史）
- 纯增量不碰核心路径
