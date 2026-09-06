# PRD — Nexus 一键 Docker 部署

## 1. 背景

Nexus 控制面已交付 M0–M14 共 15 个里程碑（全部合并 main）。当前验证方式是手动起 PG 容器 + 宿主机 `cargo run`，缺少标准化部署。用户要求：一键编译构建，部署到本机 Docker，提供一键部署脚本。

## 2. 目标

将 `codex-rs/nexus-control`（控制面）+ codex 引擎二进制 + pgvector Postgres 标准化为可一键拉起的 Docker 部署包。

## 3. 非目标

- 不改 codex-rs 内核任何源码（全部在 `deploy/` 目录）。
- 不做多 Pod 分布式 / K8s（留置外部环境依赖）。
- 不做 codex 引擎的从源码全量编译（复用已验证的引擎二进制）。

## 4. 功能需求

| FR | 描述 |
|---|---|
| FR1 | 一键脚本 `deploy.sh`：编译 nexus-control → 准备 codex 二进制 → 构建镜像 → compose up → 健康检查 → 汇报 |
| FR2 | Dockerfile：瘦运行时镜像，含 nexus-control + codex 两二进制 + 运行时库 |
| FR3 | docker-compose.yml：postgres(pgvector) + nexus 两服务，volume 持久化，健康检查 |
| FR4 | .env 模板：JWT secret / 管理员 / 模型 key / 池大小等（密钥不硬编码） |
| FR5 | 启动即自动迁移（M1–M14 全部 migration 幂等执行）+ seed 管理员 |

## 5. 验收标准

| AC | 验收点 |
|---|---|
| AC1 | `./deploy/deploy.sh` 单命令完成编译+构建+启动 |
| AC2 | `curl http://localhost:8765/health` 返回 ok |
| AC3 | `POST /v1/auth/login` 用 .env 管理员凭据返回 JWT |
| AC4 | PG 数据卷跨容器重启持久（`--down` 后重跑数据在） |
| AC5 | `--down` 停止保留卷；`--purge` 清空卷 |
| AC6 | 密钥仅从 .env 注入，不入镜像/脚本/记忆 |

## 6. 约束

- 安全红线：`NEXUS_MODEL_KEY` 仅环境变量读取，严禁记录/外传/写入记忆。
- 不改 codex 内核。
- 端口默认：PG 5434，nexus 8765。
