# Nexus 一键 Docker 部署

将 Nexus 控制面（`codex-rs/nexus-control`，M0–M14 全部 15 个里程碑产物）一键编译构建并部署到本机 Docker。

## 快速开始

```bash
cd ~/Nexus
# 首次：填入模型 key 等（或留空走 mock）
$EDITOR deploy/.env
./deploy/deploy.sh
```

部署完成后：

```bash
# 健康检查
curl http://localhost:8765/health   # -> ok

# 登录拿 JWT（管理员在 .env 配置）
curl -X POST http://localhost:8765/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"admin@nexus.local","password":"admin123"}'
```

## 架构

```
┌─────────────────────────────────────────────────────────┐
│  Docker (本机)                                            │
│                                                          │
│  ┌──────────────────┐        ┌────────────────────────┐  │
│  │ nexus-postgres   │        │ nexus-control          │  │
│  │ pgvector/pg16    │◄──────►│  (控制面 M0–M14)       │  │
│  │ 5434:5432        │  PG    │  - HTTP/WS :8765       │  │
│  │ vol: nexus-pg    │        │  - driver pool (codex) │  │
│  └──────────────────┘        │  - model gateway       │  │
│                              │  vol: codex-home       │  │
│                              └────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
        ▲
        │ 编译：cargo build --release -p nexus-control (宿主机)
        │ 引擎：复用 codex 二进制 (strip 后 ~297MB)
```

## 文件

| 文件 | 作用 |
|---|---|
| `deploy.sh` | 一键脚本：编译→打包→启动→健康检查→汇报 |
| `Dockerfile` | 瘦运行时镜像（debian-slim + 两二进制） |
| `docker-compose.yml` | postgres + nexus 两服务编排 |
| `.env.example` | 环境变量模板（凭据/密钥来源） |

## 命令

```bash
./deploy/deploy.sh            # 全流程（编译+构建+启动）
./deploy/deploy.sh --no-build # 跳过编译，复用 deploy/bin/ 已有二进制
./deploy/deploy.sh --down     # 停止容器（保留数据卷）
./deploy/deploy.sh --purge    # 停止+删数据卷（清空 DB）
```

## 环境变量（.env）

| 变量 | 默认 | 说明 |
|---|---|---|
| `NEXUS_JWT_SECRET` | 自动生成 | JWT 签名密钥（首次跑脚本自动填随机值） |
| `NEXUS_ADMIN_EMAIL/PASSWORD` | admin@nexus.local / admin123 | 首次启动 seed 管理员 |
| `NEXUS_POOL_SIZE` | 4 | 并发 driver 池大小 |
| `NEXUS_UPSTREAM_MODEL_URL` | 空 | 真实模型端点（留空走 mock） |
| `NEXUS_MODEL_KEY` | 空 | 模型 API key（**仅从 .env 读取**） |
| `NEXUS_MODEL` | deepseek-v4-pro | 模型 id |
| `NEXUS_SIMULATE_APPROVAL` | 0 | 1=合成审批（无需模型验证 HITL） |
| `PG_PORT` / `NEXUS_PORT` | 5434 / 8765 | 端口映射 |

## 设计决策

1. **宿主机编译 + 瘦镜像打包**（非容器内全量编译）：codex 工作区 100+ crate，
   容器内从零 release 编译耗时极长；nexus-control 是小 crate，宿主机 24 核秒级完成。
   codex 引擎二进制复用驱动 M0–M14 的同一份（strip 入镜像）。
2. **pgvector/pgvector:pg16**：M13 知识库 RAG 需 pgvector 扩展。
3. **迁移自动执行**：`nexus-control serve` 启动即跑 `run_migrations`（幂等，M1–M14 全部 migration）。
4. **数据卷持久化**：`nexus-pg-data`（DB）+ `nexus-codex-home`（config.toml + rules/）跨重启保留。
5. **密钥隔离**：`NEXUS_MODEL_KEY` 仅经 `.env` 注入 compose env，不写入镜像/脚本/记忆。

## 前置依赖

- Docker + Docker Compose v2（脚本自检）
- Rust 工具链（cargo，用于编译 nexus-control；或用 `--no-build` 复用二进制）
- codex 引擎二进制（默认 `~/.local/bin/codex`，可用 `NEXUS_CODEX_BIN_PATH` 覆盖）
