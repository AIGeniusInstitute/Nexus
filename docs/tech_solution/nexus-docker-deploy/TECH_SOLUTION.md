# 技术方案 — Nexus 一键 Docker 部署

## 1. 总体架构

```
┌──────────────────────── Docker (本机) ────────────────────────┐
│                                                                │
│  nexus-postgres (pgvector/pgvector:pg16)                       │
│    ├ 5434:5432 映射                                            │
│    ├ vol nexus-pg-data 持久                                    │
│    └ healthcheck pg_isready                                   │
│                                                                │
│  nexus-control (debian:bookworm-slim + 两二进制)              │
│    ├ ENTRYPOINT: nexus-control serve                          │
│    ├ 8765:8765 映射                                            │
│    ├ env: DATABASE_URL/JWT_SECRET/MODEL_KEY/POOL_SIZE ...      │
│    ├ vol nexus-codex-home（config.toml + rules/）             │
│    ├ depends_on: postgres healthy                             │
│    └ healthcheck curl /health                                 │
│                                                                │
└────────────────────────────────────────────────────────────────┘
        ▲
        │ deploy.sh 准备
        │   ├ cargo build --release -p nexus-control (宿主机)
        │   ├ strip → deploy/bin/nexus-control
        │   ├ cp ~/.local/bin/codex → strip → deploy/bin/codex
        │   └ docker compose build + up
```

## 2. 构建策略：宿主机编译 + 瘦镜像

**决策**：不采用容器内全量 `cargo build`（codex 工作区 100+ crate，从零 release 编译 30+ 分钟且需下载 git 依赖）；改为宿主机编译 nexus-control 小 crate（24 核秒级），复用已验证的 codex 引擎二进制，COPY 进瘦镜像。

**理由**：
- nexus-control 是本仓库 M0–M14 产物（"系统"），cargo 编译快。
- codex 引擎是外部依赖（非本仓库产物），复用驱动 M0–M14 的同一二进制（strip 后 ~297MB）。
- 瘦镜像（debian-slim + ca-certificates + libsqlite3-0 + libssl3 + curl）约 400MB，构建秒级。

## 3. 关键文件

### deploy/Dockerfile
- `FROM debian:bookworm-slim`
- 安装 ca-certificates（HTTPS）/ curl（健康检查）/ libsqlite3-0（codex 运行时）/ libssl3（codex 原生 TLS）
- `COPY bin/nexus-control bin/codex` → /usr/local/bin/
- ENV NEXUS_CODEX_BIN / NEXUS_CODEX_HOME=/app/.codex
- ENTRYPOINT nexus-control

### deploy/docker-compose.yml
- postgres: pgvector/pgvector:pg16 + healthcheck + volume
- nexus: build context=deploy/ + env（DATABASE_URL/JWT_SECRET/...）+ command serve + volume + healthcheck curl /health
- `NEXUS_JWT_SECRET:?` 强校验（缺则拒绝启动）
- admin_email/admin_password 经 command flag 注入（CLI 无 env）

### deploy/deploy.sh
- 前置检查：docker / docker compose / daemon
- docker context use default（本机 Desktop context 问题）
- .env 准备：从 .env.example 复制 + JWT secret 空则 openssl rand 生成
- 二进制准备：codex（NEXUS_CODEX_BIN_PATH 或 ~/.local/bin/codex，strip）+ nexus-control（cargo release，strip）
- docker compose build + up
- 轮询 postgres healthy + nexus /health
- 汇报地址 + 凭据 + 管理命令

## 4. 端口与卷

| 服务 | 端口 | 卷 |
|---|---|---|
| postgres | 5434:5432 | nexus-pg-data |
| nexus | 8765:8765 | nexus-codex-home |

## 5. 环境变量（.env，密钥不硬编码）

| 变量 | 来源 | 说明 |
|---|---|---|
| NEXUS_JWT_SECRET | 自动生成 | 脚本空则 openssl rand -hex 32 |
| NEXUS_ADMIN_EMAIL/PASSWORD | 用户填 | seed 管理员 |
| NEXUS_MODEL_KEY | 用户填 | 模型 API key（仅 .env，红线） |
| NEXUS_UPSTREAM_MODEL_URL | 用户填 | 真实模型端点（空=mock） |
| NEXUS_POOL_SIZE | 默认 4 | 并发 driver 池 |

## 6. 风险与对策

| 风险 | 对策 |
|---|---|
| codex 二进制缺失 | 脚本校验 NEXUS_CODEX_BIN_PATH / ~/.local/bin/codex，缺则报错指路 |
| docker context 指向 Desktop | 脚本 `docker context use default` |
| JWT secret 默认值 | 脚本检测默认值自动生成随机 |
| 首次编译慢 | nexus-control 小 crate，24 核秒级；可 --no-build 复用 |
