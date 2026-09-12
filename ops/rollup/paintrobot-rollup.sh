#!/usr/bin/env bash
# 오늘치(그리고 어제치) 일별 집계를 다시 계산해 daily_rollup에 넣는다.
#
# 집계(stats·mixflow)는 수집할 때마다 게이트웨이가 한 대씩 더해 두므로 평소에는
# 이 스크립트가 없어도 최신이다. 여기서는 두 가지를 맡는다:
#   1. 정합 추정(reconcile) — 지연 오프셋을 하루 전체에서 추정하는 값이라
#      한 대씩 더할 수 없다.
#   2. 증분 누적의 교정 — CoreDB에 CAS가 없어 동시 수집이 겹치면 한 건이 샐 수
#      있다. 여기서 전체를 다시 계산해 덮는다.
# 이 스캔이 도는 동안(약 8초) 다른 조회가 대기하므로 주기를 짧게 잡지 않는다.
#
# 어제치도 같이 도는 이유: 자정 직후 들어온 늦은 이벤트가 반영되게 하려고.
set -uo pipefail

API="${PAINTROBOT_API:-http://127.0.0.1:18080}"
COREDB="${COREDB_URL:-http://127.0.0.1:9043}"
ENV_FILE="${PAINTROBOT_ENV:-/home/root1/Work/Paintrobot/.env.local}"

KEY="$(grep -oP 'EDGE_API_KEYS=\K[^\s,]+' "$ENV_FILE" | head -1)"
if [ -z "$KEY" ]; then
    echo "EDGE_API_KEYS를 $ENV_FILE 에서 찾지 못했습니다" >&2
    exit 1
fi

for offset in 0 1; do
    date_str="$(date -d "-$offset day" +%F)"
    resp="$(curl -s -m 600 -X POST -H "X-Edge-Key: $KEY" \
        "$API/api/v1/stats/rollup?date=$date_str" || true)"
    echo "$(date -Is) rollup $date_str: ${resp:-(무응답)}"
done

# memtable에만 있으면 재시작 때 사라진다. 실제로 롤업 한 건을 그렇게 잃었다.
curl -s -m 600 -X POST "$COREDB/flush" >/dev/null || true
