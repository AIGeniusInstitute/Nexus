#!/usr/bin/env bash
# M12 CI eval gate (roadmap M8 T8-3).
# Runs an eval case end-to-end against a Nexus control-plane instance and
# exits non-zero if any case fails. Designed for SIMULATE mode (no real model
# needed) but works against a real-model instance too.
#
# Env:
#   NEXUS_BASE (default http://127.0.0.1:9898)
#   NEXUS_EMAIL / NEXUS_PASSWORD (admin creds)
#   NEXUS_SIMULATE_APPROVAL — if set, turns use the simulated driver (no model key)
set -euo pipefail

BASE="${NEXUS_BASE:-http://127.0.0.1:9898}"
EMAIL="${NEXUS_EMAIL:-admin@test.com}"
PASS="${NEXUS_PASSWORD:-admin123}"

echo "[eval-gate] logging in as $EMAIL ..."
TOKEN=$(curl -s -X POST "$BASE/v1/auth/login" -H 'Content-Type: application/json' \
    -d "{\"email\":\"$EMAIL\",\"password\":\"$PASS\"}" \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
AUTH="Authorization: Bearer $TOKEN"

# 1. Create (or reuse) an eval case expecting a completed turn.
CASE_ID=$(curl -s -X POST "$BASE/v1/evals/cases" -H "$AUTH" -H 'Content-Type: application/json' \
    -d '{"name":"ci-smoke","category":"smoke","input":"eval gate smoke","expected_status":"completed"}' \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
echo "[eval-gate] case_id=$CASE_ID"

# 2. Create a thread + start a turn (SIMULATE completes without real model).
TID=$(curl -s -X POST "$BASE/v1/threads" -H "$AUTH" -H 'Content-Type: application/json' \
    -d '{"title":"eval-gate"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
TURN_ID=$(curl -s -X POST "$BASE/v1/threads/$TID/turns" -H "$AUTH" -H 'Content-Type: application/json' \
    -d '{"input":"eval gate smoke"}' \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["turn_id"])')
echo "[eval-gate] turn_id=$TURN_ID"

# 3. Run the eval assertion.
PASSED=$(curl -s -X POST "$BASE/v1/evals/runs/$CASE_ID" -H "$AUTH" -H 'Content-Type: application/json' \
    -d "{\"turn_id\":$TURN_ID}" \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["passed"])')
echo "[eval-gate] passed=$PASSED"

if [ "$PASSED" = "True" ]; then
    echo "[eval-gate] PASS"
    exit 0
else
    echo "[eval-gate] FAIL"
    exit 1
fi
