#!/usr/bin/env bash
# 오늘치(그리고 어제치) 일별 집계를 다시 계산해 daily_rollup에 넣는다.
#
# 왜 필요한가: 대시보드가 읽는 값은 전부 일별 집계인데, `jobs`의 PK가
# event_id라 `WHERE work_date=...` 조회가 전체 스캔이다. 그 스캔을 요청마다
# 치르면 큐가 포화돼 서비스가 멈춘다 (실제로 그랬다). 여기서 주기적으로 한 번만
# 치르고 결과를 날짜당 한 행에 담아두면, 조회는 단일 키 조회가 된다.
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
