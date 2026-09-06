# Nexus M13 任务状态 — 知识库 RAG（pgvector）

## 里程碑
M13 = 知识库 RAG（pgvector），roadmap M11 T11-2（知识库/RAG）+ T11-3（向量库 pgvector），均自建。
分支：`feat/nexus-m13`，base 含 M12 merge `7b1f174`。

## 任务清单
| 任务 | 状态 | 说明 |
|---|---|---|
| T13-1 migration kb_documents | ✅ | CREATE EXTENSION vector + kb_documents(tenant_id/embedding vector(1024)/...) + HNSW cosine 索引 + knowledge_bases ALTER 加 tenant_id/description |
| T13-2 kb.rs 模块 | ✅ | embed()(dashscope text-embedding-v3 1024 维) + create_kb/list_kbs/ingest_document/list_documents/delete_document/search(混合召回+溯源) + vec_literal() 手动构造 pgvector 字符串 |
| T13-3 http_server 路由 | ✅ | POST/GET /v1/kbs + POST/GET /v1/kbs/{id}/documents + DELETE /v1/kbs/{id}/documents/{did} + POST /v1/kbs/{id}/search |
| T13-4 main.rs 标题 | ✅ | "Nexus M13: serve" |
| T13-5 测试 + e2e | ✅ | cargo test 30/30(+2 kb)，e2e AC13.1-13.6 全过 |

## 关键决策
1. **权限过滤先于召回**：`WHERE tenant_id=$X` 在 `ORDER BY embedding <=>` 之前，pgvector HNSW 支持过滤 ANN
2. **ACL 随索引写入**：kb_documents.tenant_id 在摄入时写入（召回时不 JOIN knowledge_bases→workspaces）
3. **冗余 tenant_id**：避免 ANN 时 JOIN（性能 + 简单）
4. **embedding 复用 M8 env**：NEXUS_UPSTREAM_MODEL_URL/NEXUS_MODEL_KEY + NEXUS_EMBED_MODEL(default text-embedding-v3)
5. **混合召回最小实现**：向量 ANN + 可选 keyword ILIKE（title OR content）；rerank 留扩展（向量距离排序足够 MVP）
6. **溯源**：返回 source_uri + title + snippet(LEFT content 200) + score(1-cosine_distance)
7. **手动 vec_literal 不引 sqlx-pgvector**：`'[0.1,0.2,...]'::vector` 字符串 bind，避免 workspace 加 feature（Surgical）
8. **content_hash 用 std DefaultHasher**：dedup/debug 非安全场景，避免加 md5 依赖
9. **KB 是 tenant-scoped（非 audit 跨租户）**：即使 admin 也只能看本租户 KB（数据隔离，与 audit 管理可见性不同）

## 坑
1. **keyword 初版只过滤 content**：doc3 title="K8s 云部署" 但 content 用 "Kubernetes"→搜 "K8s" 空。改进为 `title ILIKE OR content ILIKE`
2. **f-string 反斜杠**：bash 内联 python f-string 不能含 `\"`→改 % 格式或 helper 脚本
3. **PG 容器无 pgvector**：nexus-pg-m4 原是 postgres:16-alpine→换 pgvector/pgvector:pg16 镜像重建
4. **eval-gate 真实模型 turn 慢**：deepseek-v4-pro 是 reasoning 模型，turn >120s→零回归改用只读 API 验证（audit/timeline/eval list，不起 turn）

## 验证
- cargo check：0 error 0 warning
- cargo test：30/30（M12 28 + kb 2：vec_literal_format/embed_dim_constant）零回归
- e2e（PG nexus-pg-m4 pgvector/pgvector:pg16:5434 + NEXUS_EMBED_MODEL=text-embedding-v3 + 真实 dashscope key）AC13.1-13.6 全过：
  - AC13.1 pgvector 0.8.6 + kb_documents 表 + HNSW 索引（migration incl m13）
  - AC13.2 POST /v1/kbs 创建(id=1) + GET /v1/kbs 列表(tenant_id=1)
  - AC13.3 POST /v1/kbs/1/documents 摄入 3 篇（真实 embedding 1024 维落库，tenant_id 随索引写入，tokens 落库）
  - AC13.4 POST /v1/kbs/1/search 混合召回：语义"向量检索相关文档召回"→doc2 RAG score=0.6105 最高 + 溯源(source_uri docs/rag.md)；keyword "K8s"→doc3(title 匹配)
  - AC13.5 跨租户隔离：tenant-999 机密文档（同 embedding）对 tenant-1 admin 不可见（WHERE tenant_id 预过滤）
  - AC13.6 零回归：M10 audit(3 auth.login + WORM UPDATE→ERROR) + M11 timeline(64 条) + M12 eval cases(list 正常)
- 不改 codex 内核（全部 nexus-control crate）；不动既有表（knowledge_bases 用 ALTER；新建 kb_documents）
