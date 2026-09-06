#!/usr/bin/env bash
# Nexus 一键部署脚本
# 功能：编译 nexus-control → 准备 codex 引擎二进制 → 构建 Docker 镜像 →
#       docker compose up → 等待健康 → 打印服务地址与凭据。
#
# 用法：
#   ./deploy/deploy.sh            # 全流程
#   ./deploy/deploy.sh --no-build # 跳过宿主机编译（用已有 deploy/bin/ 二进制）
#   ./deploy/deploy.sh --down     # 停止并清理容器（保留数据卷）
#   ./deploy/deploy.sh --purge    # 停止并删除数据卷（清空 DB）
#
# 凭据：从 deploy/.env 读取（首次自动从 .env.example 复制）。
# 红线：NEXUS_MODEL_KEY 仅经 .env/环境传入，脚本绝不记录/外传。

set -euo pipefail

# ---------- 路径与颜色 ----------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CODEX_RS="$REPO_ROOT/codex-rs"
BIN_DIR="$SCRIPT_DIR/bin"
ENV_FILE="$SCRIPT_DIR/.env"
ENV_EXAMPLE="$SCRIPT_DIR/.env.example"

RED=$'\033[31m'; GRN=$'\033[32m'; YEL=$'\033[33m'; CYN=$'\033[36m'; RST=$'\033[0m'
log()  { printf "${CYN}[deploy]${RST} %s\n" "$*"; }
ok()   { printf "${GRN}[ok]${RST} %s\n" "$*"; }
warn() { printf "${YEL}[warn]${RST} %s\n" "$*"; }
err()  { printf "${RED}[err]${RST} %s\n" "$*" >&2; }

# ---------- 参数 ----------
ACTION="up"
NO_BUILD=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build) NO_BUILD=1; shift ;;
    --down)     ACTION="down"; shift ;;
    --purge)    ACTION="purge"; shift ;;
    -h|--help)
      sed -n '2,18p' "$0"; exit 0 ;;
    *) err "未知参数: $1"; exit 2 ;;
  esac
done

# ---------- 前置检查 ----------
if ! command -v docker >/dev/null 2>&1; then
  err "未找到 docker。请先安装并启动 Docker。"; exit 1
fi
if ! docker info >/dev/null 2>&1; then
  err "docker daemon 未运行（或当前用户无权访问 docker.sock）。"
  warn "若 systemd 未启动 docker：sudo systemctl start docker.socket docker"
  exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
  err "未找到 docker compose 子命令（需 Docker Compose v2）。"; exit 1
fi

# 切换 docker context（本机默认可能指向 Desktop）
if docker context ls --format '{{.Name}}' 2>/dev/null | grep -qx default; then
  docker context use default >/dev/null 2>&1 || true
fi

# ---------- down / purge ----------
if [[ "$ACTION" == "down" ]]; then
  log "停止 Nexus 容器（保留数据卷）..."
  (cd "$SCRIPT_DIR" && docker compose --env-file .env down)
  ok "已停止。数据卷保留，再次 ./deploy.sh 即可恢复。"
  exit 0
fi
if [[ "$ACTION" == "purge" ]]; then
  warn "将删除 nexus-pg-data 与 nexus-codex-home 数据卷（DB 清空）！5 秒后开始，Ctrl+C 取消..."
  sleep 5
  (cd "$SCRIPT_DIR" && docker compose --env-file .env down -v)
  ok "已清理。"
  exit 0
fi

# ---------- .env ----------
if [[ ! -f "$ENV_FILE" ]]; then
  if [[ -f "$ENV_EXAMPLE" ]]; then
    cp "$ENV_EXAMPLE" "$ENV_FILE"
    warn "已从 .env.example 创建 .env，请编辑填入真实凭据后重跑：${EDITOR:-vi} $ENV_FILE"
  fi
fi
# 必须存在 .env 才能读密钥（compose --env-file 强制）
if [[ ! -f "$ENV_FILE" ]]; then
  err "缺少 $ENV_FILE。请手动创建。"; exit 1
fi
# 强校验 JWT secret 未改默认值
JWT_SEC=$(grep -E '^NEXUS_JWT_SECRET=' "$ENV_FILE" | head -1 | cut -d= -f2- | tr -d '[:space:]')
if [[ -z "$JWT_SEC" || "$JWT_SEC" == "change-me-to-a-long-random-secret" ]]; then
  warn "NEXUS_JWT_SECRET 仍是默认值/空，正在生成随机密钥写入 .env ..."
  NEW_SEC="$(openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | xxd -p -c 64)"
  sed -i "s|^NEXUS_JWT_SECRET=.*|NEXUS_JWT_SECRET=$NEW_SEC|" "$ENV_FILE"
  ok "已生成随机 JWT secret 并写入 .env"
fi
ok ".env 就绪"

# ---------- 编译 + 准备二进制 ----------
mkdir -p "$BIN_DIR"

# 1) codex 引擎二进制：优先 NEXUS_CODEX_BIN_PATH 环境变量，其次 ~/.local/bin/codex
CODEX_SRC="${NEXUS_CODEX_BIN_PATH:-$HOME/.local/bin/codex}"
if [[ ! -x "$CODEX_SRC" ]]; then
  err "未找到 codex 引擎二进制（$CODEX_SRC）。"
  warn "请设 NEXUS_CODEX_BIN_PATH 指向 codex 可执行文件，或安装到 ~/.local/bin/codex。"
  exit 1
fi
log "准备 codex 引擎二进制（strip 减小体积）..."
if [[ "$CODEX_SRC" -ef "$BIN_DIR/codex" ]]; then
  : # 已就位
else
  cp "$CODEX_SRC" "$BIN_DIR/codex"
fi
strip "$BIN_DIR/codex" 2>/dev/null || warn "strip codex 失败（非致命，镜像略大）"
chmod +x "$BIN_DIR/codex"
ok "codex 引擎就位: $(du -h "$BIN_DIR/codex" | cut -f1)"

# 2) nexus-control 控制面：宿主机 cargo release 编译
NEEDLE_BIN="$BIN_DIR/nexus-control"
if [[ "$NO_BUILD" -eq 1 && -x "$NEEDLE_BIN" ]]; then
  ok "跳过编译（--no-build），使用已有 $NEEDLE_BIN"
else
  if ! command -v cargo >/dev/null 2>&1; then
    err "未找到 cargo。请先安装 Rust 工具链（rustup.rs），或用 --no-build 复用已有二进制。"
    exit 1
  fi
  log "编译 nexus-control release（宿主机，24 核可用，首次较慢）..."
  (cd "$CODEX_RS" && cargo build --release -p nexus-control --bin nexus-control)
  cp "$CODEX_RS/target/release/nexus-control" "$NEEDLE_BIN"
  strip "$NEEDLE_BIN" 2>/dev/null || true
  chmod +x "$NEEDLE_BIN"
  ok "nexus-control 编译完成: $(du -h "$NEEDLE_BIN" | cut -f1)"
fi

# 3) M19: stdio MCP echo server（测试 fixture）
cp "$CODEX_RS/nexus-control/tests/mcp_echo_server.py" "$BIN_DIR/mcp_echo_server.py" 2>/dev/null || warn "mcp_echo_server.py 未找到（非致命）"
chmod +x "$BIN_DIR/mcp_echo_server.py" 2>/dev/null || true

# 4) Web 控制台静态产物：npm install + vite build → 复制到上下文 web-dist/
WEB_SRC="$CODEX_RS/nexus-control/web"
WEB_DIST_CTX="$SCRIPT_DIR/web-dist"
if [[ "$NO_BUILD" -eq 1 && -d "$WEB_DIST_CTX" && -f "$WEB_DIST_CTX/index.html" ]]; then
  ok "跳过前端构建（--no-build），复用已有 $WEB_DIST_CTX"
else
  if ! command -v npm >/dev/null 2>&1; then
    err "未找到 npm。请先安装 Node.js（含 npm），或用 --no-build 复用已构建产物。"
    exit 1
  fi
  log "构建 Web 控制台（npm install + vite build）..."
  (cd "$WEB_SRC" && env -u NODE_ENV npm install --include=dev --no-audit --no-fund)
  # esbuild postinstall 可能被 allow-scripts 拦截，手动补装二进制
  if [[ -f "$WEB_SRC/node_modules/esbuild/install.js" ]] && ! "$WEB_SRC/node_modules/.bin/esbuild" --version >/dev/null 2>&1; then
    node "$WEB_SRC/node_modules/esbuild/install.js" 2>/dev/null || true
  fi
  (cd "$WEB_SRC" && env -u NODE_ENV npm run build)
  rm -rf "$WEB_DIST_CTX"
  mkdir -p "$WEB_DIST_CTX"
  cp -r "$WEB_SRC/dist/." "$WEB_DIST_CTX/"
  ok "Web 控制台产物就位: $(du -sh "$WEB_DIST_CTX" | cut -f1)"
fi

# ---------- Docker 构建 + 启动 ----------
log "构建 Docker 镜像 nexus-control:latest ..."
(cd "$SCRIPT_DIR" && docker compose --env-file .env build)

log "启动服务（postgres + nexus）..."
(cd "$SCRIPT_DIR" && docker compose --env-file .env up -d)

# ---------- 等待健康 ----------
log "等待 postgres 健康..."
for i in $(seq 1 60); do
  st=$(docker inspect --format '{{.State.Health.Status}}' nexus-postgres 2>/dev/null || echo "none")
  [[ "$st" == "healthy" ]] && { ok "postgres healthy"; break; }
  sleep 2
  [[ $i -eq 60 ]] && { err "postgres 健康检查超时"; docker logs nexus-postgres 2>&1 | tail -20; exit 1; }
done

log "等待 nexus-control 健康（/health）..."
for i in $(seq 1 60); do
  if curl -fsS "http://localhost:${NEXUS_PORT:-8765}/health" >/dev/null 2>&1; then
    ok "nexus-control /health ok"; break
  fi
  sleep 2
  [[ $i -eq 60 ]] && {
    err "nexus-control 健康检查超时。最近日志："
    docker logs nexus-control 2>&1 | tail -40
    exit 1
  }
done

# ---------- 汇报 ----------
NEXUS_PORT_VAL="${NEXUS_PORT:-8765}"
ADMIN_EMAIL=$(grep -E '^NEXUS_ADMIN_EMAIL=' "$ENV_FILE" | head -1 | cut -d= -f2-)
echo
ok "================ Nexus 部署完成 ================"
printf "  控制面:   ${GRN}http://localhost:%s${RST}\n" "$NEXUS_PORT_VAL"
printf "  健康检查: http://localhost:%s/health\n" "$NEXUS_PORT_VAL"
printf "  管理员:   %s\n" "$ADMIN_EMAIL"
echo "  API 示例:"
echo "    curl -X POST http://localhost:${NEXUS_PORT_VAL}/v1/auth/login \\"
echo "      -H 'Content-Type: application/json' \\"
echo "      -d '{\"email\":\"${ADMIN_EMAIL}\",\"password\":\"<your-password>\"}'"
echo
printf "  管理:\n"
printf "    查看日志: docker logs -f nexus-control\n"
printf "    停止:     %s --down\n" "$0"
printf "    清空数据: %s --purge\n" "$0"
echo "=================================================="
