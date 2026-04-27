#!/usr/bin/env bash
# Probe paint.coreon.build/healthz; restart the tunnel after 2 consecutive
# failures (cloudflared sometimes goes zombie — process alive, edge connections dead).
set -uo pipefail

URL="${WATCHDOG_URL:-https://paint.coreon.build/healthz}"
TIMEOUT="${WATCHDOG_TIMEOUT:-8}"
STATE="${WATCHDOG_STATE:-/tmp/paintrobot_tunnel_fail_count}"
FAIL_THRESHOLD="${WATCHDOG_FAIL_THRESHOLD:-2}"

now() { date -Is; }

if curl -sf -m "$TIMEOUT" "$URL" >/dev/null; then
    echo 0 > "$STATE"
    exit 0
fi

prev="$(cat "$STATE" 2>/dev/null || echo 0)"
n=$((prev + 1))
echo "$n" > "$STATE"

echo "$(now) tunnel probe failed ($n/$FAIL_THRESHOLD)" >&2

if [ "$n" -ge "$FAIL_THRESHOLD" ]; then
    echo "$(now) restarting paintrobot-tunnel.service" >&2
    systemctl --user restart paintrobot-tunnel.service
    echo 0 > "$STATE"
fi
