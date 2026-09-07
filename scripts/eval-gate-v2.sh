#!/bin/bash
# M21 CI 质量门：跑批量评测运行 → 质量阈值判定 → P0 阻断 / P1 警告
# 用法：./scripts/eval-gate-v2.sh
# 环境变量：
#   NEXUS_BASE (默认 http://localhost:8765)
#   NEXUS_EMAIL / NEXUS_PASSWORD (默认 admin@nexus.local / admin123)
#   NEXUS_EVAL_P0_THRESHOLD (默认 0.6，accuracy 低于此值 → P0 阻断 exit 1)
#   NEXUS_EVAL_P1_DELTA (默认 0.1，与 baseline accuracy 下降超过此值 → P1 警告)
#   NEXUS_EVAL_BASELINE_RUN_ID (可选，baseline run id 用于对比)
#   NEXUS_SIMULATE_JUDGE (默认 1，SIMULATE 模式不调真实模型)
set -euo pipefail

BASE="${NEXUS_BASE:-http://localhost:8765}"
EMAIL="${NEXUS_EMAIL:-admin@nexus.local}"
PASSWORD="${NEXUS_PASSWORD:-admin123}"
P0="${NEXUS_EVAL_P0_THRESHOLD:-0.6}"
P1="${NEXUS_EVAL_P1_DELTA:-0.1}"
BASELINE="${NEXUS_EVAL_BASELINE_RUN_ID:-}"

echo "=== Nexus M21 CI 质量门 ==="
echo "P0 阈值(accuracy): $P0"
echo "P1 阈值(delta):    $P1"
echo "baseline run:      ${BASELINE:-无}"

# 1. login
TOKEN=$(curl -s -X POST "$BASE/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
AUTH="Authorization: Bearer $TOKEN"
echo "[1] login ok"

# 2. 查找已发布的 golden set
GS_LIST=$(curl -s "$BASE/v1/golden-sets" -H "$AUTH")
GS_ID=$(echo "$GS_LIST" | python3 -c '
import sys,json
d=json.load(sys.stdin)
# 找第一个 locked 的
for gs in d:
    if gs.get("status")=="locked" and gs.get("active_version_id"):
        print(gs["id"]); break
else:
    print("")
')
if [ -z "$GS_ID" ]; then
  echo "[ERR] 无已发布的 golden set，CI 门禁失败"
  exit 1
fi
echo "[2] golden_set_id=$GS_ID"

# 3. 创建 thread
TID=$(curl -s -X POST "$BASE/v1/threads" -H "$AUTH" -H 'Content-Type: application/json' -d '{}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
echo "[3] thread_id=$TID"

# 4. 查找已发布的 judge config（可选）
JC_ID=""
JC_LIST=$(curl -s "$BASE/v1/judge-configs" -H "$AUTH")
JC_ID=$(echo "$JC_LIST" | python3 -c '
import sys,json
d=json.load(sys.stdin)
for jc in d:
    if jc.get("status")=="locked" and jc.get("active_version_id"):
        print(jc["id"]); break
else:
    print("")
')
echo "[4] judge_config_id=${JC_ID:-无}"

# 5. 启动批量运行
BODY="{\"golden_set_id\":$GS_ID,\"thread_id\":\"$TID\""
if [ -n "$JC_ID" ]; then
  BODY="$BODY,\"judge_config_id\":$JC_ID"
fi
BODY="$BODY}"
RUN_ID=$(curl -s -X POST "$BASE/v1/evals/batch-runs" -H "$AUTH" -H 'Content-Type: application/json' -d "$BODY" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["run_id"])')
echo "[5] run_id=$RUN_ID"

# 6. 查 aggregate
sleep 3
RUN=$(curl -s "$BASE/v1/evals/batch-runs/$RUN_ID" -H "$AUTH")
STATUS=$(echo "$RUN" | python3 -c 'import sys,json;print(json.load(sys.stdin)["run"]["status"])')
if [ "$STATUS" != "completed" ]; then
  echo "[WARN] run status=$STATUS，等待..."
  sleep 5
  RUN=$(curl -s "$BASE/v1/evals/batch-runs/$RUN_ID" -H "$AUTH")
fi

ACCURACY=$(echo "$RUN" | python3 -c 'import sys,json;print(json.load(sys.stdin)["run"]["aggregate"]["accuracy"])')
PASSED=$(echo "$RUN" | python3 -c 'import sys,json;print(json.load(sys.stdin)["run"]["aggregate"]["passed"])')
TOTAL=$(echo "$RUN" | python3 -c 'import sys,json;print(json.load(sys.stdin)["run"]["aggregate"]["total_cases"])')
JUDGE=$(echo "$RUN" | python3 -c 'import sys,json;print(json.load(sys.stdin)["run"]["aggregate"].get("avg_judge_score",0))')

echo "[6] 结果: accuracy=$ACCURACY passed=$PASSED/$TOTAL avg_judge=$JUDGE"

# 7. P0 阻断判定
P0_FAIL=$(python3 -c "print(1 if float('$ACCURACY') < float('$P0') else 0)")
if [ "$P0_FAIL" = "1" ]; then
  echo "❌ P0 阻断: accuracy=$ACCURACY < 阈值 $P0"
  echo "CI_GATE=P0_BLOCKED"
  exit 1
fi
echo "✅ P0 通过: accuracy=$ACCURACY >= 阈值 $P0"

# 8. P1 警告判定（与 baseline 对比）
if [ -n "$BASELINE" ]; then
  BASE_RUN=$(curl -s "$BASE/v1/evals/batch-runs/$BASELINE" -H "$AUTH")
  BASE_ACC=$(echo "$BASE_RUN" | python3 -c 'import sys,json;print(json.load(sys.stdin)["run"]["aggregate"]["accuracy"])')
  DELTA=$(python3 -c "print(float('$BASE_ACC') - float('$ACCURACY'))")
  P1_WARN=$(python3 -c "print(1 if float('$DELTA') > float('$P1') else 0)")
  if [ "$P1_WARN" = "1" ]; then
    echo "⚠ P1 警告: accuracy 下降 $DELTA (baseline=$BASE_ACC → current=$ACCURACY) > 阈值 $P1"
    echo "CI_GATE=P1_WARN"
    exit 0
  fi
  echo "✅ P1 通过: accuracy delta=$DELTA <= 阈值 $P1 (baseline=$BASE_ACC)"
fi

echo "CI_GATE=PASS"
exit 0
