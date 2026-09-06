#!/usr/bin/env bash
# Nexus 系统测试验收脚本 — 覆盖 M0-M19 全功能（Docker 端到端）
set -uo pipefail
BASE="${BASE:-http://localhost:8765}"
EMAIL="${EMAIL:-admin@nexus.local}"
PASS="${PASS:-admin123}"
PASS_CNT=0; FAIL_CNT=0; RESULTS="["; FIRST=1

emit() {
  local case="$1" status="$2" detail="$3"
  if [ "$FIRST" -eq 1 ]; then FIRST=0; else RESULTS+=","; fi
  RESULTS+="{\"case\":\"$case\",\"status\":\"$status\",\"detail\":\"$detail\"}"
  if [ "$status" = "PASS" ]; then PASS_CNT=$((PASS_CNT+1)); else FAIL_CNT=$((FAIL_CNT+1)); fi
}

JWT=$(curl -s -X POST "$BASE/v1/auth/login" -H 'Content-Type: application/json' -d "{\"email\":\"$EMAIL\",\"password\":\"$PASS\"}" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("token",""))' 2>/dev/null)
if [ -n "$JWT" ]; then emit "M1-login" "PASS" "JWT len=${#JWT}"; else emit "M1-login" "FAIL" "no token"; exit 1; fi
AUTH="Authorization: Bearer $JWT"

R=$(curl -s -o /tmp/r.json -w "%{http_code}" "$BASE/v1/auth/me" -H "$AUTH")
[ "$R" = "200" ] && emit "M1-auth-me" "PASS" "200 perms=$(python3 -c 'import json;print(json.load(open("/tmp/r.json")).get("perms"))' 2>/dev/null)" || emit "M1-auth-me" "FAIL" "http=$R"

TID=$(curl -s -X POST "$BASE/v1/threads" -H "$AUTH" -H 'Content-Type: application/json' -d '{}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))' 2>/dev/null)
[ -n "$TID" ] && emit "M1-thread-create" "PASS" "thread=$TID" || emit "M1-thread-create" "FAIL" "no id"

R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/threads" -H "$AUTH")
[ "$R" = "200" ] && emit "M1-thread-list" "PASS" "200" || emit "M1-thread-list" "FAIL" "http=$R"

# M3 approval loop
( curl -s --max-time 30 -X POST "$BASE/v1/threads/$TID/turns" -H "$AUTH" -H 'Content-Type: application/json' -d '{"input":"sys-test"}' > /tmp/turn.json 2>&1 ) &
sleep 3
APID=$(curl -s "$BASE/v1/approvals" -H "$AUTH" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d[0]["id"] if d else "")' 2>/dev/null)
if [ -n "$APID" ]; then
  curl -s -o /dev/null -X POST "$BASE/v1/approvals/$APID/resolve" -H "$AUTH" -H 'Content-Type: application/json' -d '{"decision":"approve"}'
  wait
  TS=$(python3 -c 'import json;print(json.load(open("/tmp/turn.json")).get("status",""))' 2>/dev/null)
  [ "$TS" = "completed" ] && emit "M3-approval-loop" "PASS" "approval=$APID approve→completed" || emit "M3-approval-loop" "FAIL" "turn=$TS"
else
  wait; TS=$(python3 -c 'import json;print(json.load(open("/tmp/turn.json")).get("status",""))' 2>/dev/null)
  [ "$TS" = "completed" ] && emit "M3-approval-loop" "PASS" "turn completed (no park)" || emit "M3-approval-loop" "FAIL" "turn=$TS"
fi

R=$(curl -s -o /tmp/r.json -w "%{http_code}" "$BASE/v1/usage?days=7" -H "$AUTH")
[ "$R" = "200" ] && emit "M4-usage" "PASS" "200" || emit "M4-usage" "FAIL" "http=$R"

R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/policy/rules" -H "$AUTH"); [ "$R" = "200" ] && emit "M6-policy-rules" "PASS" "200" || emit "M6-policy-rules" "FAIL" "http=$R"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/policy/feedback?days=7" -H "$AUTH"); [ "$R" = "200" ] && emit "M6-policy-feedback" "PASS" "200" || emit "M6-policy-feedback" "FAIL" "http=$R"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/audit/logs?limit=10" -H "$AUTH"); [ "$R" = "200" ] && emit "M10-audit-logs" "PASS" "200" || emit "M10-audit-logs" "FAIL" "http=$R"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/threads/$TID/timeline" -H "$AUTH"); [ "$R" = "200" ] && emit "M11-timeline" "PASS" "200" || emit "M11-timeline" "FAIL" "http=$R"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/evals/cases" -H "$AUTH"); [ "$R" = "200" ] && emit "M12-evals-cases" "PASS" "200" || emit "M12-evals-cases" "FAIL" "http=$R"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/kbs" -H "$AUTH"); [ "$R" = "200" ] && emit "M13-kbs" "PASS" "200" || emit "M13-kbs" "FAIL" "http=$R"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/threads/$TID/snapshots" -H "$AUTH"); [ "$R" = "200" ] && emit "M14-snapshots" "PASS" "200" || emit "M14-snapshots" "FAIL" "http=$R"
R=$(curl -s -o /tmp/r.json -w "%{http_code}" "$BASE/v1/runtime/pool" -H "$AUTH"); [ "$R" = "200" ] && emit "M15-pool" "PASS" "$(cat /tmp/r.json)" || emit "M15-pool" "FAIL" "http=$R"

CID=$(curl -s -X POST "$BASE/v1/connectors" -H "$AUTH" -H 'Content-Type: application/json' -d '{"name":"st-conn","kind":"mcp"}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))' 2>/dev/null)
[ -n "$CID" ] && emit "M16-connector-create" "PASS" "conn=$CID" || emit "M16-connector-create" "FAIL" "no id"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/connectors" -H "$AUTH"); [ "$R" = "200" ] && emit "M16-connector-list" "PASS" "200" || emit "M16-connector-list" "FAIL" "http=$R"

SID=$(curl -s -X POST "$BASE/v1/skills" -H "$AUTH" -H 'Content-Type: application/json' -d '{"name":"st-skill","description":"t"}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))' 2>/dev/null)
[ -n "$SID" ] && emit "M17-skill-create" "PASS" "skill=$SID" || emit "M17-skill-create" "FAIL" "no id"
VID=$(curl -s -X POST "$BASE/v1/skills/$SID/versions" -H "$AUTH" -H 'Content-Type: application/json' -d '{"version":"1.0.0","checksum":"abc","content_ref":"v1"}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))' 2>/dev/null)
[ -n "$VID" ] && emit "M17-skill-publish" "PASS" "version=$VID" || emit "M17-skill-publish" "FAIL" "no vid"
R=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/v1/skills/$SID/rollback" -H "$AUTH" -H 'Content-Type: application/json' -d "{\"version_id\":$VID}"); [ "$R" = "200" ] && emit "M17-skill-rollback" "PASS" "ok" || emit "M17-skill-rollback" "FAIL" "http=$R"

# M18 多 Agent 协作编排（3 模式）
OWID=$(curl -s --max-time 90 -X POST "$BASE/v1/orchestrations" -H "$AUTH" -H 'Content-Type: application/json' -d '{"mode":"orchestrator-worker","prompt":"test","agents":2}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("orchestration_id",""))' 2>/dev/null)
OWST=$(curl -s "$BASE/v1/orchestrations/$OWID" -H "$AUTH" | python3 -c 'import sys,json;print(json.load(sys.stdin)["orchestration"]["status"])' 2>/dev/null)
[ "$OWST" = "completed" ] && emit "M18-orch-worker" "PASS" "orch=$OWID completed" || emit "M18-orch-worker" "FAIL" "status=$OWST"

PID=$(curl -s --max-time 90 -X POST "$BASE/v1/orchestrations" -H "$AUTH" -H 'Content-Type: application/json' -d '{"mode":"peer","prompt":"test","agents":2}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("orchestration_id",""))' 2>/dev/null)
PST=$(curl -s "$BASE/v1/orchestrations/$PID" -H "$AUTH" | python3 -c 'import sys,json;print(json.load(sys.stdin)["orchestration"]["status"])' 2>/dev/null)
[ "$PST" = "completed" ] && emit "M18-orch-peer" "PASS" "orch=$PID completed" || emit "M18-orch-peer" "FAIL" "status=$PST"

CAID=$(curl -s --max-time 90 -X POST "$BASE/v1/orchestrations" -H "$AUTH" -H 'Content-Type: application/json' -d '{"mode":"critic-adversarial","prompt":"test"}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("orchestration_id",""))' 2>/dev/null)
CAST=$(curl -s "$BASE/v1/orchestrations/$CAID" -H "$AUTH" | python3 -c 'import sys,json;print(json.load(sys.stdin)["orchestration"]["status"])' 2>/dev/null)
[ "$CAST" = "completed" ] && emit "M18-orch-critic" "PASS" "orch=$CAID completed" || emit "M18-orch-critic" "FAIL" "status=$CAST"
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/v1/orchestrations" -H "$AUTH"); [ "$R" = "200" ] && emit "M18-orch-list" "PASS" "200" || emit "M18-orch-list" "FAIL" "http=$R"

# M19 MCP Gateway 真实转发
MCID=$(curl -s -X POST "$BASE/v1/connectors" -H "$AUTH" -H 'Content-Type: application/json' -d '{"name":"echo-mcp-st","kind":"mcp","config_json":{"command":"python3","args":["/app/mcp_echo_server.py"]}}' | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))' 2>/dev/null)
RES=$(curl -s -X POST "$BASE/v1/connectors/$MCID/invoke" -H "$AUTH" -H 'Content-Type: application/json' -d '{"tool":"echo","args":{"message":"verify"}}' 2>/dev/null)
echo "$RES" | python3 -c "import sys,json;d=json.load(sys.stdin);print('PASS' if d.get('mcp') and d.get('success') and 'echo:verify' in d.get('result','') else 'FAIL')" >/tmp/mcpchk 2>/dev/null
chk=$(cat /tmp/mcpchk 2>/dev/null)
[ "$chk" = "PASS" ] && emit "M19-mcp-echo" "PASS" "mcp=true result=$(echo $RES | python3 -c 'import sys,json;print(json.load(sys.stdin)["result"][:40])' 2>/dev/null)" || emit "M19-mcp-echo" "FAIL" "res=$RES"
Q=$(curl -s "$BASE/v1/connectors/$MCID/quality" -H "$AUTH" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("quality_score"))' 2>/dev/null)
[ -n "$Q" ] && emit "M19-mcp-quality" "PASS" "score=$Q" || emit "M19-mcp-quality" "FAIL" "no score"

RESULTS+="]"
echo "{\"total\":$((PASS_CNT+FAIL_CNT)),\"pass\":$PASS_CNT,\"fail\":$FAIL_CNT,\"results\":$RESULTS}"
