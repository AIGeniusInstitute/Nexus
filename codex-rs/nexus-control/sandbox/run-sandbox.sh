#!/bin/bash
# Nexus T0-5 三层沙箱启动 + T0-6 自检驱动脚本（宿主侧）
# 验证 H3：容器层 + OS 层(seccomp/capdrop) + 网络层 三层隔离生效
set -uo pipefail  # 不用 -e：selfcheck/网络探测允许非零退出，手动判断
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROFILE="$SCRIPT_DIR/seccomp-profile.json"
IMAGE="nexus-sandbox:latest"

# ── 构建镜像 ──
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "[build] 构建 $IMAGE ..."
  docker build -q -t "$IMAGE" -f "$SCRIPT_DIR/Dockerfile" "$SCRIPT_DIR" >/dev/null
fi

echo "============================================================"
echo "[AC5.1 + AC5.3 + seccomp + cgroup] --network none --read-only 自检"
echo "============================================================"
# network none: 完全无网络（AC5.1 ping 必拒）
# read-only: 只读 rootfs（AC 自检 item4）
# seccomp profile: 禁逃逸 syscall（item1）
# cap-drop ALL + no-new-privileges: 最小特权
# cpus/memory/pids-limit: cgroup 限额（item5）
docker run --rm \
  --network none \
  --read-only \
  --tmpfs /tmp:rw,size=10m \
  --tmpfs /home/nexus:rw,size=20m \
  --security-opt seccomp="$PROFILE" \
  --security-opt no-new-privileges \
  --cap-drop ALL \
  --user nexus \
  --cpus=1 \
  --memory=256m \
  --pids-limit=64 \
  "$IMAGE" /opt/selfcheck.sh
RC=$?
echo "[selfcheck exit=$RC] (0=通过可调度, 非0=拒绝调度)"

echo "============================================================"
echo "[AC5.2] 自定义网络 + iptables 出站白名单（仅放行 Model Gateway）"
echo "============================================================"
# 创建仅容器间的 internal bridge 网络（默认隔离外网，AC5.1 双保险）
docker network inspect nexus-sandbox-net >/dev/null 2>&1 \
  || docker network create --internal nexus-sandbox-net >/dev/null

# Model Gateway 地址（T0-7 联调时由 gateway 容器注入；PoC 先验证隔离原语）
GATEWAY_ADDR="${GATEWAY_ADDR:-127.0.0.1:8080}"
echo "(Model Gateway 预期地址: $GATEWAY_ADDR — 待 T0-7 完成后联调 AC5.2)"

docker run --rm \
  --network nexus-sandbox-net \
  --read-only --tmpfs /tmp:rw,size=10m --tmpfs /home/nexus:rw,size=20m \
  --security-opt seccomp="$PROFILE" --security-opt no-new-privileges \
  --cap-drop ALL --user nexus --cpus=1 --memory=256m --pids-limit=64 \
  "$IMAGE" sh -c '
    echo "uid=$(id -u) rootfs=$(touch /__p 2>/dev/null && echo writable || echo readonly)";
    if ping -c1 -W2 8.8.8.8 >/dev/null 2>&1; then
      echo "FAIL: 外网可达（隔离失效）"; exit 2
    else
      echo "PASS: 外网被拒（internal 网络隔离生效）"
    fi
  '
echo "[AC5.2 exit=$?]"

echo "============================================================"
echo "[done] T0-5 三层沙箱原语验证完成"
