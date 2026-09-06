# Nexus 控制面运行时镜像（瘦镜像）
#
# 设计：在宿主机上用 cargo 编译 nexus-control release 二进制 + 复用已 strip
# 的 codex 引擎二进制，由 deploy.sh 准备到 deploy/bin/ 后，本 Dockerfile 仅做
# COPY 打包成可运行镜像。这避免了在容器内重编整个 codex 工作区（100+ crate，
# 耗时极长），保持镜像瘦（~400MB）且构建秒级。
#
# codex 二进制是外部引擎依赖（非本仓库 M0–M14 产物），nexus-control 才是
# 本仓库的"系统"。codex 复用驱动 M0–M14 的同一引擎二进制（strip 后 ~297MB）。

FROM debian:bookworm-slim

# ca-certificates: reqwest rustls HTTPS；curl: 容器内健康检查；
# libsqlite3-0: codex 引擎运行时；libssl3: codex 原生 TLS。
# python3: M19 MCP Gateway 真实转发测试 fixture（stdio MCP echo server）。
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libsqlite3-0 libssl3 python3 \
    && rm -rf /var/lib/apt/lists/*

# deploy.sh 预先把两个二进制放到 ./bin/ 上下文
COPY bin/nexus-control /usr/local/bin/nexus-control
COPY bin/codex /usr/local/bin/codex
RUN chmod +x /usr/local/bin/nexus-control /usr/local/bin/codex

# M19: 内置 stdio MCP echo server，供连接器真实转发测试。
COPY bin/mcp_echo_server.py /app/mcp_echo_server.py
RUN chmod +x /app/mcp_echo_server.py

# Web 控制台静态产物（deploy.sh 构建 web/dist 后复制到上下文 web-dist/）
COPY web-dist/ /app/web-dist/

# codex_home：config.toml + rules/ + 子进程工作目录（volume 持久化）
RUN mkdir -p /app/.codex
ENV NEXUS_CODEX_BIN=/usr/local/bin/codex
ENV NEXUS_CODEX_HOME=/app/.codex
ENV NEXUS_WEB_DIR=/app/web-dist
WORKDIR /app

EXPOSE 8765

# ENTRYPOINT 固定为 serve；flag 由 compose command 注入（含 admin 凭据）。
ENTRYPOINT ["/usr/local/bin/nexus-control"]
