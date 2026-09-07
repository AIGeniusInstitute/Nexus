#!/usr/bin/env bash
# =============================================================================
# Nexus 一键运维部署脚本（ops.sh）
# -----------------------------------------------------------------------------
# 覆盖 Nexus 控制面全生命周期运维：部署 / 启停 / 状态 / 日志 / 备份恢复 /
# 升级 / 前端热更 / 数据库操作 / 系统测试 / 环境自检。
#
# 架构：nexus-control 控制面 + codex 引擎（容器内）+ Postgres(pgvector)。
#   部署细节（编译+打包+启动）委托 deploy/deploy.sh，本脚本聚焦运维动作。
#
# 用法：
#   ./deploy/ops.sh up            # 全流程部署（编译+构建镜像+启动+健康）
#   ./deploy/ops.sh up --no-build # 复用已有二进制，跳过编译
#   ./deploy/ops.sh down          # 停止容器（保留数据卷）
#   ./deploy/ops.sh restart       # 重启 nexus 控制面（不动 Postgres）
#   ./deploy/ops.sh purge         # 停止+删除数据卷（清空 DB，慎用）
#   ./deploy/ops.sh status        # 容器状态 + 健康 + driver 池 + 用量概览
#   ./deploy/ops.sh health        # 仅健康检查
#   ./deploy/ops.sh logs [svc]    # 跟随日志（nexus|postgres，默认 nexus）
#   ./deploy/ops.sh psql [sql]    # 进入 psql 或执行单条 SQL
#   ./deploy/ops.sh shell         # 进入 nexus 容器 shell
#   ./deploy/ops.sh backup [file] # pg_dump 逻辑备份到 downloads/
#   ./deploy/ops.sh restore <f>  # 从备份文件恢复
#   ./deploy/ops.sh upgrade       # git pull + 重新编译 + 重建 + 重启
#   ./deploy/ops.sh rebuild-web   # 仅重建前端并热替换（不重启后端）
#   ./deploy/ops.sh sys-test      # 运行端到端系统测试（25 用例）
#   ./deploy/ops.sh env-check     # 前置依赖与配置自检
#   ./deploy/ops.sh help          # 显示本帮助
#
# 红线：NEXUS_MODEL_KEY 仅经 .env/环境注入，脚本绝不记录/外传。
# =============================================================================

set -euo pipefail

# ---------- 路径与常量 ----------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEPLOY_DIR="$SCRIPT_DIR"
ENV_FILE="$DEPLOY_DIR/.env"
DEPLOY_SH="$DEPLOY_DIR/deploy.sh"
CODEX_RS="$REPO_ROOT/codex-rs"
WEB_SRC="$CODEX_RS/nexus-control/web"
DOWNLOADS_DIR="$REPO_ROOT/downloads"

# 容器与服务名（与 docker-compose.yml 一致）
PG_CONTAINER="nexus-postgres"
NX_CONTAINER="nexus-control"
COMPOSE_SERVICES=(postgres nexus)

# ---------- 颜色 ----------
RED=$'\033[31m'; GRN=$'\033[32m'; YEL=$'\033[33m'; CYN=$'\033[36m'; DIM=$'\033[2m'; RST=$'\033[0m'
log()  { printf "${CYN}[ops]${RST} %s\n" "$*"; }
ok()   { printf "${GRN}[ok]${RST} %s\n" "$*"; }
warn() { printf "${YEL}[warn]${RST} %s\n" "$*"; }
err()  { printf "${RED}[err]${RST} %s\n" "$*" >&2; }
section() { printf "\n${CYN}══ %s ══${RST}\n" "$*"; }

# ---------- 工具函数 ----------
# 读取 .env 变量（容错：未配置返回默认）
env_val() {
  local key="$1" def="${2:-}"
  local v
  v=$(grep -E "^${key}=" "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2- || true)
  echo "${v:-$def}"
}

# 确认危险操作
confirm() {
  local msg="$1"
  printf "${YEL}%s${RST} 输入 yes 继续: " "$msg"
  read -r ans
  [[ "$ans" == "yes" || "$ans" == "y" ]] || { err "已取消"; exit 1; }
}

# docker compose 包装（带 .env）
dc() {
  (cd "$DEPLOY_DIR" && docker compose --env-file .env "$@")
}

# 取 nexus 端口
nx_port() { env_val NEXUS_PORT 8765; }

# 取 admin 凭据
admin_email() { env_val NEXUS_ADMIN_EMAIL admin@nexus.local; }
admin_pass()  { env_val NEXUS_ADMIN_PASSWORD admin123; }

# 取 PG 凭据
pg_user() { env_val POSTGRES_USER nexus; }
pg_db()   { env_val POSTGRES_DB nexus; }

# curl 带超时
curlt() { curl --max-time 5 -fsS "$@"; }

# 取容器状态
is_running() { docker inspect -f '{{.State.Running}}' "$1" 2>/dev/null | grep -q true; }

# 登录拿 JWT（内部用）
auth_token() {
  local port email pass
  port=$(nx_port); email=$(admin_email); pass=$(admin_pass)
  curlt -X POST "http://localhost:${port}/v1/auth/login" \
    -H 'Content-Type: application/json' \
    -d "{\"email\":\"${email}\",\"password\":\"${pass}\"}" 2>/dev/null \
    | sed -n 's/.*"token"[:[:space:]]*"\([^"]*\)".*/\1/p'
}

# ---------- 子命令：env-check ----------
cmd_env_check() {
  section "环境依赖自检"
  local fail=0

  check() {
    local name="$1" cmd="$2" hint="$3"
    if command -v "$cmd" >/dev/null 2>&1; then
      printf "  ${GRN}✓${RST} %-12s %s\n" "$name" "$($cmd --version 2>&1 | head -1)"
    else
      printf "  ${RED}✗${RST} %-12s %s\n" "$name" "${hint:-未安装}"
      fail=1
    fi
  }

  check "docker"  docker  "需安装 Docker + Compose v2：sudo systemctl start docker.socket docker"
  check "cargo"  cargo   "需 Rust 工具链：curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  check "npm"    npm     "需 Node.js（含 npm）：https://nodejs.org"
  check "openssl" openssl "用于生成 JWT secret"
  if command -v strip >/dev/null 2>&1; then
    printf "  ${GRN}✓${RST} %-12s %s\n" "strip" "binutils"
  else
    printf "  ${YEL}!${RST} %-12s %s\n" "strip" "缺失（非致命，镜像略大）"
  fi

  # codex 引擎二进制
  local codex_bin="${NEXUS_CODEX_BIN_PATH:-$HOME/.local/bin/codex}"
  if [[ -x "$codex_bin" ]]; then
    printf "  ${GRN}✓${RST} %-12s %s (%s)\n" "codex" "引擎二进制就位" "$(du -h "$codex_bin" | cut -f1)"
  else
    printf "  ${RED}✗${RST} %-12s %s\n" "codex" "未找到 $codex_bin，设 NEXUS_CODEX_BIN_PATH 覆盖"
    fail=1
  fi

  # .env
  section "部署配置 (.env)"
  if [[ ! -f "$ENV_FILE" ]]; then
    printf "  ${YEL}!${RST} .env 不存在，首次部署将自动从 .env.example 创建\n"
  else
    local jwt_sec
    jwt_sec=$(env_val NEXUS_JWT_SECRET)
    if [[ -z "$jwt_sec" || "$jwt_sec" == "change-me-to-a-long-random-secret" ]]; then
      printf "  ${YEL}!${RST} NEXUS_JWT_SECRET 为默认值（部署时自动生成随机密钥）\n"
    else
      printf "  ${GRN}✓${RST} NEXUS_JWT_SECRET 已设置 (len=%s)\n" "${#jwt_sec}"
    fi
    local mk
    mk=$(env_val NEXUS_MODEL_KEY)
    [[ -z "$mk" ]] && printf "  ${DIM}○${RST} NEXUS_MODEL_KEY 空 → 走 mock 模型（turn 仍可完成）\n" \
                      || printf "  ${GRN}✓${RST} NEXUS_MODEL_KEY 已配置\n"
  fi

  # 端口占用
  section "端口占用"
  local pg_port nx_port_val
  pg_port=$(env_val PG_PORT 5435); nx_port_val=$(nx_port)
  for p in "$pg_port" "$nx_port_val"; do
    if ss -tlnp 2>/dev/null | grep -q ":${p} " || docker port "$PG_CONTAINER" 2>/dev/null | grep -q ":${p}->" ; then
      printf "  ${GRN}✓${RST} 端口 %s（被本服务/已映射）\n" "$p"
    else
      printf "  ${DIM}○${RST} 端口 %s 空闲\n" "$p"
    fi
  done

  echo
  if [[ $fail -eq 0 ]]; then ok "环境自检通过，可执行 ./deploy/ops.sh up"; else err "自检未全通过，按提示补齐后重试"; exit 1; fi
}

# ---------- 子命令：up（委托 deploy.sh）----------
cmd_up() {
  exec "$DEPLOY_SH" "$@"
}

# ---------- 子命令：down ----------
cmd_down() {
  section "停止 Nexus 容器（保留数据卷）"
  dc down
  ok "已停止。数据卷保留，./deploy/ops.sh up 即可恢复。"
}

# ---------- 子命令：restart ----------
cmd_restart() {
  section "重启 nexus 控制面"
  if ! is_running "$NX_CONTAINER"; then err "$NX_CONTAINER 未运行，请先 ./deploy/ops.sh up"; exit 1; fi
  docker restart "$NX_CONTAINER"
  log "等待 /health ..."
  for i in $(seq 1 30); do
    curlt "http://localhost:$(nx_port)/health" >/dev/null 2>&1 && { ok "nexus 已恢复 (尝试 $i)"; exit 0; }
    sleep 2
  done
  err "重启后健康检查超时，查看日志：./deploy/ops.sh logs"
  exit 1
}

# ---------- 子命令：purge ----------
cmd_purge() {
  warn "将删除 nexus-pg-data 与 nexus-codex-home 数据卷（DB 清空、rules 重置）！"
  confirm "确认清空全部数据？"
  dc down -v
  ok "已清理，可 ./deploy/ops.sh up 全新部署。"
}

# ---------- 子命令：health ----------
cmd_health() {
  local port; port=$(nx_port)
  section "健康检查"
  if curlt "http://localhost:${port}/health" 2>/dev/null; then
    echo; ok "nexus-control /health ok"
  else
    err "nexus-control 不可达（端口 ${port}）。容器状态："
    docker ps -a --filter "name=$NX_CONTAINER" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
    exit 1
  fi
}

# ---------- 子命令：status ----------
cmd_status() {
  section "容器状态"
  docker ps -a --filter "name=nexus-" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" 2>/dev/null || warn "docker 不可用"

  local port; port=$(nx_port)
  section "控制面健康"
  if curlt "http://localhost:${port}/health" >/dev/null 2>&1; then
    ok "/health ok (port ${port})"
  else
    err "/health 不可达"
  fi

  # 需鉴权的概览
  local token
  token=$(auth_token 2>/dev/null || true)
  if [[ -n "$token" ]]; then
    section "Driver 池"
    curlt -H "Authorization: Bearer $token" "http://localhost:${port}/v1/runtime/pool" 2>/dev/null \
      | sed 's/^/  /' || warn "池状态获取失败"
    section "近 7 日用量"
    curlt -H "Authorization: Bearer $token" "http://localhost:${port}/v1/usage?days=7" 2>/dev/null \
      | sed 's/^/  /' || warn "用量获取失败"
    section "待审批"
    curlt -H "Authorization: Bearer $token" "http://localhost:${port}/v1/approvals" 2>/dev/null \
      | sed 's/^/  /' || warn "审批列表获取失败"
  else
    warn "无法登录获取 JWT（admin 凭据可能不符），鉴权概览跳过"
  fi
}

# ---------- 子命令：logs ----------
cmd_logs() {
  local svc="${1:-nexus}"
  case "$svc" in
    nexus|nx) svc="$NX_CONTAINER" ;;
    pg|postgres|db) svc="$PG_CONTAINER" ;;
  esac
  log "跟随 $svc 日志（Ctrl+C 退出）"
  docker logs -f --tail 200 "$svc"
}

# ---------- 子命令：psql ----------
cmd_psql() {
  if ! is_running "$PG_CONTAINER"; then err "$PG_CONTAINER 未运行"; exit 1; fi
  if [[ $# -gt 0 ]]; then
    docker exec -i "$PG_CONTAINER" psql -U "$(pg_user)" -d "$(pg_db)" -c "$*"
  else
    log "进入 psql 交互（\\q 退出）"
    docker exec -it "$PG_CONTAINER" psql -U "$(pg_user)" -d "$(pg_db)"
  fi
}

# ---------- 子命令：shell ----------
cmd_shell() {
  if ! is_running "$NX_CONTAINER"; then err "$NX_CONTAINER 未运行"; exit 1; fi
  log "进入 $NX_CONTAINER shell（exit 退出）"
  docker exec -it "$NX_CONTAINER" bash
}

# ---------- 子命令：backup ----------
cmd_backup() {
  if ! is_running "$PG_CONTAINER"; then err "$PG_CONTAINER 未运行，无法备份"; exit 1; fi
  mkdir -p "$DOWNLOADS_DIR"
  local ts file
  ts=$(date +%Y%m%d-%H%M%S)
  file="${1:-$DOWNLOADS_DIR/nexus-backup-$ts.sql.gz}"
  section "逻辑备份 → $file"
  docker exec "$PG_CONTAINER" pg_dump -U "$(pg_user)" -d "$(pg_db)" --clean --if-exists 2>/dev/null \
    | gzip > "$file"
  local sz; sz=$(du -h "$file" | cut -f1)
  ok "备份完成: $file ($sz)"
  warn "注：仅 PG 逻辑备份。codex-home（config.toml/rules）在 nexus-codex-home 卷，如需一并备份：docker run --rm -v nexus-codex-home:/data -v $DOWNLOADS_DIR:/out alpine tar czf /out/codex-home-$ts.tgz -C /data ."
}

# ---------- 子命令：restore ----------
cmd_restore() {
  local file="${1:-}"
  [[ -z "$file" || ! -f "$file" ]] && { err "用法: ops.sh restore <backup.sql[.gz]>"; exit 2; }
  if ! is_running "$PG_CONTAINER"; then err "$PG_CONTAINER 未运行"; exit 1; fi
  warn "将从 $file 恢复，会覆盖当前 DB 数据！"
  confirm "确认恢复？"
  section "恢复中..."
  case "$file" in
    *.gz) gzip -dc "$file" | docker exec -i "$PG_CONTAINER" psql -U "$(pg_user)" -d "$(pg_db)" 2>&1 | sed 's/^/  /' ;;
    *)   cat "$file" | docker exec -i "$PG_CONTAINER" psql -U "$(pg_user)" -d "$(pg_db)" 2>&1 | sed 's/^/  /' ;;
  esac
  ok "恢复完成。建议重启控制面：./deploy/ops.sh restart"
}

# ---------- 子命令：upgrade ----------
cmd_upgrade() {
  section "拉取最新代码"
  (cd "$REPO_ROOT" && git fetch --all && git pull --ff-only origin main 2>/dev/null || git pull --ff-only)
  section "重新部署（编译 + 重建镜像 + 滚动重启）"
  "$DEPLOY_SH"
  ok "升级完成"
}

# ---------- 子命令：rebuild-web（前端热更）----------
cmd_rebuild_web() {
  section "构建 Web 控制台"
  if ! command -v npm >/dev/null 2>&1; then err "未找到 npm"; exit 1; fi
  (cd "$WEB_SRC" && env -u NODE_ENV npm install --include=dev --no-audit --no-fund)
  if [[ -f "$WEB_SRC/node_modules/esbuild/install.js" ]] && ! "$WEB_SRC/node_modules/.bin/esbuild" --version >/dev/null 2>&1; then
    node "$WEB_SRC/node_modules/esbuild/install.js" 2>/dev/null || true
  fi
  (cd "$WEB_SRC" && env -u NODE_ENV npm run build)
  local web_dist="$DEPLOY_DIR/web-dist"
  rm -rf "$web_dist"; mkdir -p "$web_dist"
  cp -r "$WEB_SRC/dist/." "$web_dist/"
  ok "Web 产物就绪: $(du -sh "$web_dist" | cut -f1)"

  if is_running "$NX_CONTAINER"; then
    section "热替换到运行容器"
    docker cp "$web_dist/." "$NX_CONTAINER:/app/web-dist/"
    ok "已热替换，刷新浏览器即可（无需重启后端）"
    warn "注：镜像未更新，重建容器后会回退；如需持久化镜像请跑 ./deploy/ops.sh upgrade"
  else
    warn "容器未运行，仅更新构建产物；下次 up 生效"
  fi
}

# ---------- 子命令：sys-test ----------
cmd_sys_test() {
  local script="$REPO_ROOT/scripts/sys-test.sh"
  if [[ ! -x "$script" ]]; then err "未找到 $script"; exit 1; fi
  section "端到端系统测试（M0-M19，25 用例）"
  "$script" "$@"
}

# ---------- 帮助 ----------
cmd_help() {
  sed -n '3,33p' "$0"
}

# ---------- 入口 ----------
case "${1:-help}" in
  up)           shift; cmd_up "$@" ;;
  down)         cmd_down ;;
  restart)      cmd_restart ;;
  purge)        cmd_purge ;;
  status|st)    cmd_status ;;
  health)       cmd_health ;;
  logs)         shift; cmd_logs "$@" ;;
  psql)         shift; cmd_psql "$@" ;;
  shell)        cmd_shell ;;
  backup)       shift; cmd_backup "$@" ;;
  restore)      shift; cmd_restore "$@" ;;
  upgrade)      cmd_upgrade ;;
  rebuild-web)  cmd_rebuild_web ;;
  sys-test)     shift; cmd_sys_test "$@" ;;
  env-check)    cmd_env_check ;;
  help|-h|--help) cmd_help ;;
  *) err "未知子命令: $1"; echo "运行 ./deploy/ops.sh help 查看用法"; exit 2 ;;
esac
