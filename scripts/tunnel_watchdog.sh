#!/usr/bin/env bash
# Probe cloudflared's local /ready endpoint and restart the tunnel if it
# reports zero healthy edge connections for FAIL_THRESHOLD consecutive checks.
#
# We deliberately use the LOCAL metrics port (127.0.0.1:20243) rather than the
# public hostname:
#  1. The original symptom we want to catch is "cloudflared is alive but
#     edge connections went zombie" — `readyConnections` reflects exactly that.
#  2. Probing the public hostname would also trip on egress firewall changes
#     and DNS hiccups that have nothing to do with the tunnel itself, causing
#     unnecessary restarts and a flap loop.
set -uo pipefail

URL="${WATCHDOG_URL:-http://127.0.0.1:20243/ready}"
TIMEOUT="${WATCHDOG_TIMEOUT:-3}"
STATE="${WATCHDOG_STATE:-/tmp/paintrobot_tunnel_fail_count}"
FAIL_THRESHOLD="${WATCHDOG_FAIL_THRESHOLD:-2}"

now() { date -Is; }

resp="$(curl -s -m "$TIMEOUT" "$URL" 2>/dev/null || true)"
ready="$(printf '%s' "$resp" | grep -oE '"readyConnections":[0-9]+' | grep -oE '[0-9]+$')"

if [ -n "$ready" ] && [ "$ready" -gt 0 ]; then
    echo 0 > "$STATE"
    exit 0
fi

prev="$(cat "$STATE" 2>/dev/null || echo 0)"
n=$((prev + 1))
echo "$n" > "$STATE"

echo "$(now) cloudflared not ready (readyConnections=${ready:-?}) ($n/$FAIL_THRESHOLD)" >&2

if [ "$n" -ge "$FAIL_THRESHOLD" ]; then
    echo "$(now) restarting paintrobot-tunnel.service" >&2
    systemctl --user restart paintrobot-tunnel.service
    echo 0 > "$STATE"
fi
