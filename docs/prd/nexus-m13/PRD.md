# Nexus M13 PRD — 知识库 RAG（pgvector）

## 背景
roadmap M11 阶段⑤「规模化与生态」的 T11-2（知识库/RAG）+ T11-3（向量库 pgvector），均标 `self`/自建。M0–M12 治理阶段完成后，知识库 RAG 是剩余自主可完成维度之一（不依赖外部凭证：pgvector 扩展已就绪，embedding API 用既有 dashscope key 可调）。

初始 migration 已建空壳 `knowledge_bases` 表（id/workspace_id/name/acl_json/created_at），从未接线。M13 补齐 RAG 全链路。

## 目标
为 Nexus 控制面补齐**知识库 RAG**：文档摄入（embedding 索引）→ 混合召回（向量 ANN + 关键词过滤）→ 溯源（返回 source/title/snippet）。权限过滤先于召回（roadmap T11-3 验收标准）。

## 范围（MVP）

### 功能
1. **知识库 CRUD**：创建/列表 KB（租户隔离，KB 归属 workspace）
2. **文档摄入**：POST 文档（title + content + source_uri）→ 调 dashscope text-embedding-v3 生成 1024 维向量 → 存 `kb_documents`（tenant_id 随索引写入）
3. **混合召回搜索**：query → embed → pgvector HNSW 余弦 ANN + tenant_id 预过滤（权限先于召回）+ 可选关键词 ILIKE 过滤 → top-k + 溯源（source_uri/title/snippet/score）

### 非目标（留扩展）
- rerank 模型（需额外 API 调用，speculative，M13 用向量距离排序足够）
- 细粒度 ACL（角色/用户级）——M13 用 tenant_id 硬隔离（已验证的多租户边界）；acl_json 列保留供未来扩展
- 文档分块（chunking）——M13 单文档单向量；分块策略留扩展
- 增量更新/删除（M13 支持 delete by id，但不做版本化）

## 验收标准
- AC13.1 pgvector 扩展 + kb_documents 表 + HNSW 索引建立
- AC13.2 POST /v1/kbs 创建 KB + GET /v1/kbs 列表（租户隔离）
- AC13.3 POST /v1/kbs/{id}/documents 摄入文档（embedding 1024 维落库，tenant_id 随索引写入）
- AC13.4 POST /v1/kbs/{id}/search 混合召回：返回 top-k + 溯源（source_uri/title/snippet/score）
- AC13.5 权限过滤先于召回：跨租户 KB/文档不可见（search 返回空）
- AC13.6 零回归：M12 eval + M11 timeline + M10 audit 仍工作

## 约束
- 不改 codex 内核（全部 nexus-control crate）
- 不动既有表（knowledge_bases 用 ALTER 加 tenant_id 列；新建 kb_documents）
- embedding key 仅从 env（NEXUS_MODEL_KEY），绝不硬编码/日志/记忆
