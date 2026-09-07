#!/usr/bin/env bash
# 在 Linux 容器内编译 codex + nexus-control，产出 aarch64 Linux ELF 二进制到 deploy/bin/
#
# 背景：deploy.sh 的"宿主机编译"假定宿主机是 Linux。macOS 上 cargo 产出 Mach-O，
#       而 nexus 容器是 Linux(aarch64)，导致 `exec format error`。
#       本脚本用 rust:1.95-bookworm 镜像在 Linux 容器内编译，复用宿主机的 cargo
#       缓存(registry + git)走 --offline，避免 GitHub 拉取的网络抖动。
#
# 用法: ./deploy/build-linux-binaries.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CODEX_RS="$REPO_ROOT/codex-rs"
TARGET_DIR="$SCRIPT_DIR/target-linux"
BIN_DIR="$SCRIPT_DIR/bin"
IMAGE="rust:1.95-bookworm"

mkdir -p "$TARGET_DIR" "$BIN_DIR"

echo "[build-linux] 在容器内编译 (${IMAGE}) ..."

docker run --rm \
  -v "$CODEX_RS":/build:ro \
  -v "$HOME/.cargo/registry":/usr/local/cargo/registry \
  -v "$HOME/.cargo/git":/usr/local/cargo/git \
  -v "$HOME/.cargo/config.toml":/usr/local/cargo/config.toml \
  -v "$TARGET_DIR":/target \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_HOME=/usr/local/cargo \
  -e RUSTUP_TOOLCHAIN=1.95.0-aarch64-unknown-linux-gnu \
  "$IMAGE" \
  bash -c '
    set -e
    echo "=== apt 切换国内镜像 + 安装原生构建依赖 ==="
    sed -i "s|deb.debian.org|mirrors.tuna.tsinghua.edu.cn|g" /etc/apt/sources.list.d/debian.sources
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends \
      cmake pkg-config libssl-dev libsqlite3-dev clang perl >/dev/null
    echo "=== 原生依赖就绪 ==="

    # git 依赖拉取兜底(若 cargo 意外重拉 git 依赖, 用与宿主机一致的抗抖配置)
    git config --global http.version HTTP/1.1
    git config --global http.lowSpeedLimit 5000
    git config --global http.lowSpeedTime 20

    cd /build
    echo "=== [1/2] 编译 codex (codex-cli) ==="
    cargo build --release --locked -j 8 --bin codex -p codex-cli
    echo "=== [2/2] 编译 nexus-control ==="
    cargo build --release --locked -j 8 --bin nexus-control -p nexus-control

    echo "=== strip ==="
    strip /target/release/codex /target/release/nexus-control 2>/dev/null || true
    echo "=== 产物 ==="
    file /target/release/codex /target/release/nexus-control
    ls -lh /target/release/codex /target/release/nexus-control
  '

echo "[build-linux] 拷贝产物到 deploy/bin/ ..."
cp "$TARGET_DIR/release/codex" "$BIN_DIR/codex"
cp "$TARGET_DIR/release/nexus-control" "$BIN_DIR/nexus-control"
chmod +x "$BIN_DIR/codex" "$BIN_DIR/nexus-control"
echo "[build-linux] 完成:"
file "$BIN_DIR/codex" "$BIN_DIR/nexus-control"
