#!/usr/bin/env bash
# A scripted run of the normal-transfer workflow from the command line.
# Usage: scripts/demo-curl.sh   (backend must be running)
set -euo pipefail
BASE="${FINGUARD_URL:-http://127.0.0.1:8080}"
json() { python3 -c "import sys,json; d=json.load(sys.stdin); print($1)"; }
echo "== propose"
RES=$(curl -fsS -X POST "$BASE/api/v1/agent/commands" -H 'content-type: application/json' -H "Idempotency-Key: demo-$(date +%s)" -d '{"text":"Send ₱2,000 to Alex."}')
echo "$RES" | json "d['status'], d['policy_decision']"
ACT=$(echo "$RES" | json "d['action_id']"); CNF=$(echo "$RES" | json "d['confirmation']['confirmation_id']")
echo "== confirm (key abc-$ACT)"
curl -fsS -X POST "$BASE/api/v1/actions/$ACT/confirm" -H 'content-type: application/json' -H "Idempotency-Key: abc-$ACT" -d "{\"confirmation_id\":\"$CNF\"}" | json "d['status'], d['result']"
echo "== retry with the same key (lost response) -> replayed"
curl -fsS -X POST "$BASE/api/v1/actions/$ACT/confirm" -H 'content-type: application/json' -H "Idempotency-Key: abc-$ACT" -d "{\"confirmation_id\":\"$CNF\"}" | json "d['status'], 'replayed=', d['replayed']"
echo "== trace"
curl -fsS "$BASE/api/v1/actions/$ACT/trace" | json "[e['label'] for e in d['events']]"
