# Nexus M22 事件溯源 + Lineage + 电子签名 — 任务状态

## 范围
M20 评测中心 P3 阶段：事件溯源 + as-of 回放 + Lineage 血缘 + 电子签名（21 CFR Part 11）+ CAS/WORM 对象存储。

## 任务清单
- [x] T22-1 migration `20260906000016_m22_event_esign_cas.sql`：domain_events + esign_records + content_store 3 张 WORM 表 + raise_append_only 函数 + 3 trigger
- [x] T22-2 domain_events.rs：record_event（best-effort + event_seq MAX+1 原子 INSERT）+ list_events（as_of 过滤）+ replay_state（应用层 merge）+ 2 单测
- [x] T22-3 esign.rs：sign（SHA-256 绑定 signer/entity/version/purpose/meaning/timestamp）+ verify（重算 hash 比对 + 三要素完整性）+ list + 1 单测
- [x] T22-4 content_store.rs：store_content（SHA-256 去重 ON CONFLICT）+ get_content + resolve_ref（cas: 前缀解引用）+ store_text + 2 单测
- [x] T22-5 lineages.rs：lineage_for_run（SQL JOIN 拼装 eval_batch_run → golden_set/version/cases/results/events/esign 图）
- [x] T22-6 http_server.rs：7 路由（events/lineage/esign×4/content）+ 5 事件埋点（gs_publish/jc_publish/eval_batch_start/approval_resolve/esign_sign）
- [x] T22-7 Web GoldenSets.tsx LineageTab + api.ts 接口

## 编译验证
- cargo check: 0 error 0 warning
- cargo test: 45/45（M21 40 + 5 新：domain_events 2 + esign 1 + content_store 2）
- tsc exit 0, vite build
- Docker 部署成功

## e2e 验证（SIMULATE_JUDGE=1 + SIMULATE_APPROVAL=1）
- AC22.1 事件落库（golden_set publish → domain_events seq=1 published）✅
- AC22.2 domain_events WORM（DELETE → 拒绝）✅
- AC22.3 as-of 回放（replay=true → replayed_state 含 _last_event_seq）✅
- AC22.4 Lineage（eval_batch_run → 6 nodes 5 edges：run/golden_set/version/case_result/agent_output/event）✅
- AC22.5 电子签名（21 CFR 三要素 signer+purpose+meaning+timestamp）✅
- AC22.6 esign WORM（UPDATE → 拒绝）✅
- AC22.7 CAS 写入（content_hash=efbd92... 去重）✅
- AC22.8 content_store WORM（UPDATE → 拒绝）✅
- AC22.9 CAS 引用（cas:hash ref 返回）✅
- AC22.10 签名验证（valid=True hash_match=True complete=True）✅
- AC22.11 零回归（sys-test 25/25 全 PASS）✅

## bug 修复
1. EvalBatchRunRow 已在 M21 加 judge_version_id（M22 复用）
2. domain_events.rs anyhow! 宏需 anyhow::anyhow!（非 anyhow! 直接调用）
3. esign hash 用 signed_at.to_rfc3339() 导致 PG TIMESTAMPTZ 微秒精度与 chrono 纳秒 to_rfc3339 不一致 → 改用 signed_at.timestamp()（秒级 i64）稳定
4. CREATE TRIGGER 非幂等（重跑报 trigger already exists）→ 加 DROP TRIGGER IF EXISTS ... ON table

## 状态
全部完成，待合并 main。
