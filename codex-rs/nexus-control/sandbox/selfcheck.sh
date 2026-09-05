#!/bin/bash
# Nexus T0-6 沙箱启动自检（容器内执行）
# 5 项自检，任一不过 exit 1（禁止调度）。对应 PRD AC6.1/AC6.2。
set -u
P=0; F=0
pass(){ echo "PASS $1"; P=$((P+1)); }
fail(){ echo "FAIL $1"; F=$((F+1)); }

echo "=== Nexus 沙箱启动自检 ==="

# item1: seccomp 危险 syscall 被禁（unshare 应被拒绝）
if unshare --pid true 2>/dev/null; then
  fail "item1 seccomp: unshare 未被禁（沙箱无效）"
else
  pass "item1 seccomp: 危险 syscall(unshare) 被禁"
fi

# item2: 出站仅白名单（ping 公网应失败）
if ping -c1 -W2 8.8.8.8 >/dev/null 2>&1; then
  fail "item2 出站: ping 8.8.8.8 成功（应为白名单/禁出站）"
else
  pass "item2 出站: ping 8.8.8.8 被拒（网络隔离生效）"
fi

# item3: 无长期密钥（env 扫描常见 API key）
if env | grep -qiE 'OPENAI_API_KEY|ANTHROPIC_API_KEY|sk-[a-zA-Z0-9]{20}|AWS_SECRET_ACCESS_KEY'; then
  fail "item3 密钥: env 中发现 API 密钥（禁止长期密钥入镜像）"
else
  pass "item3 密钥: env 无长期 API 密钥"
fi

# item4: 只读 rootfs + 非 root
if touch /__ro_probe 2>/dev/null; then
  rm -f /__ro_probe 2>/dev/null
  fail "item4 rootfs: / 可写（应只读）"
else
  pass "item4 rootfs: 只读 rootfs 生效"
fi
if [ "$(id -u)" -eq 0 ]; then
  fail "item4b 用户: 以 root 运行（应非 root）"
else
  pass "item4b 用户: 非 root (uid=$(id -u))"
fi

# item5: cgroup 资源限额（MEM/PID）生效——memory.max 在 --memory 限制下为数字，无限制时为 "max"
mem_limit=""
if [ -r /sys/fs/cgroup/memory.max ]; then mem_limit=$(cat /sys/fs/cgroup/memory.max 2>/dev/null); fi
pids_limit=""
if [ -r /sys/fs/cgroup/pids.max ]; then pids_limit=$(cat /sys/fs/cgroup/pids.max 2>/dev/null); fi
if [ -n "$mem_limit" ] && [ "$mem_limit" != "max" ]; then
  pass "item5 cgroup: MEM 限额=${mem_limit} bytes (pids.max=${pids_limit})"
else
  fail "item5 cgroup: 无 MEM 限额 (memory.max=${mem_limit})"
fi

echo "=== 自检汇总: PASS=$P FAIL=$F ==="
if [ "$F" -eq 0 ]; then
  echo "RESULT: ALLOW_SCHEDULABLE"
  exit 0
else
  echo "RESULT: DENY_SCHEDULABLE"
  exit 1
fi
