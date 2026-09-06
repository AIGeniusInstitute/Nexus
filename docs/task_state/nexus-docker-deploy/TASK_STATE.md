# 任务状态 — Nexus 一键 Docker 部署

## 里程碑
Nexus 一键 Docker 部署（编译构建 + 本机 Docker 部署 + 一键脚本）。
分支：`feat/nexus-docker-deploy`，base M14 merge `25103cf`。

## 任务清单
| 任务 | 状态 | 说明 |
|---|---|---|
| T1 勘察 | ✅ | codex 二进制 1.3G debug（strip 297M）/ rust 1.97.1 / docker 22.06 / 无既有 Dockerfile / native 依赖 openssl-sys+libsqlite3-sys |
| T2 Dockerfile | ✅ | 瘦运行时 debian-slim + 两二进制 COPY |
| T3 docker-compose.yml | ✅ | pgvector PG + nexus，volume + healthcheck |
| T4 .env 模板 | ✅ | JWT/admin/model key/pool（密钥不硬编码） |
| T5 deploy.sh | ✅ | 编译→打包→启动→健康检查→汇报，含 --down/--purge/--no-build |
| T6 README | ✅ | 架构图 + 命令 + 变量表 |
| T7 端到端验证 | ✅ | deploy.sh 全流程 + AC1-6 全过（health/login/持久化/purge） |

## 设计决策
1. **宿主机编译 + 瘦镜像**（非容器内全量 cargo build）：codex 工作区 100+ crate 全量 release 编译 30+ min；nexus-control 小 crate 宿主机秒级；codex 引擎复用已验证二进制
2. **pgvector/pgvector:pg16**：M13 RAG 需 pgvector 扩展
3. **迁移自动执行**：serve 启动即 run_migrations（幂等，M1-M14 全部）
4. **密钥隔离**：NEXUS_MODEL_KEY 仅 .env 注入 compose env，不入镜像/脚本/记忆
5. **JWT secret 自动生成**：检测默认值/空则 openssl rand -hex 32 写入 .env
