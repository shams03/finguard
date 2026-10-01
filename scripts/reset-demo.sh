#!/usr/bin/env bash
# Restore the seeded demo state (balance, card, recipients, history) via the API.
set -euo pipefail
BASE="${FINGUARD_URL:-http://127.0.0.1:8080}"
curl -fsS -X POST "$BASE/api/v1/demo/reset"; echo
