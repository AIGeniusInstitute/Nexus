FROM ubuntu:22.04

# Nexus M0 PoC 沙箱镜像：codex app-server 运行在非 root、只读 rootfs 容器内
# read-only rootfs 由 `docker run --read-only` 控制；seccomp/capdrop 由 run-sandbox.sh 控制

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      libssl3 liblzma5 libgcc-s1 ca-certificates \
      iputils-ping procps util-linux \
 && rm -rf /var/lib/apt/lists/*

RUN useradd -r -m -u 1000 nexus

COPY codex-sandbox /usr/local/bin/codex
RUN chmod 555 /usr/local/bin/codex

COPY selfcheck.sh /opt/selfcheck.sh
RUN chmod 555 /opt/selfcheck.sh

USER nexus
WORKDIR /home/nexus
ENV CODEX_HOME=/home/nexus/.codex

# 默认进 app-server stdio 模式；自检用 /opt/selfcheck.sh 覆盖 CMD
CMD ["codex", "app-server", "--stdio"]
