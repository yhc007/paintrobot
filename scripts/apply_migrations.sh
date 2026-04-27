#!/usr/bin/env bash
# Apply CQL migrations to CoreDB HTTP API in order.
# Not idempotent — CoreDB doesn't support IF NOT EXISTS.
# Already-applied migrations will report "already exists" errors, which are safely ignored.
set -euo pipefail

COREDB_URL="${COREDB_URL:-http://127.0.0.1:9043}"
DIR="$(cd "$(dirname "$0")/../migrations" && pwd)"

escape_json() { python3 -c 'import json,sys;print(json.dumps(sys.stdin.read().strip()))'; }

for f in "$DIR"/*.cql; do
    name="$(basename "$f")"
    cql="$(cat "$f")"
    payload="{\"query\":$(printf '%s' "$cql" | escape_json)}"
    echo ">> $name"
    resp="$(curl -s -m 15 -X POST "$COREDB_URL/query" \
        -H 'Content-Type: application/json' \
        -d "$payload")"
    echo "   $resp"
    if echo "$resp" | grep -q '"status":"error"'; then
        if echo "$resp" | grep -qiE 'already exists'; then
            echo "   (already applied — skipping)"
        else
            echo "   FAILED" >&2
            exit 1
        fi
    fi
done
echo "migrations OK"
