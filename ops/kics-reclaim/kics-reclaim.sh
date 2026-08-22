#!/usr/bin/env bash
# kics-reclaim — Kaspersky KICS가 삭제-오픈 임시파일로 붙잡은 디스크를 회수한다.
#
# KICS 스캔은 대용량 임시파일을 만든 뒤 unlink 하지만 파일핸들을 닫지 않아
# 디스크 블록이 계속 점유된다(=삭제됐지만 df엔 잡힘). rm/truncate 로는 못 없애고,
# 유일한 회수법은 프로세스가 핸들을 닫는 것 → kics.service 재시작이다.
#
# 이 스크립트는 kics 프로세스가 쥔 '삭제-오픈' 파일 총량을 재서, 임계치를 넘으면
# (그리고 그때만) kics.service 를 재시작한다. 불필요한 보안SW 재시작을 피하려고
# KICS 가 실제로 상당량을 붙잡고 있을 때만 동작한다.
set -euo pipefail

UNIT="${KICS_UNIT:-kics.service}"
THRESHOLD_GB="${KICS_RECLAIM_THRESHOLD_GB:-20}"     # 붙잡은 용량이 이 이상이면 재시작
DISK_MIN_FREE_GB="${KICS_RECLAIM_MIN_FREE_GB:-25}"  # 보조: / 여유가 이 미만 + held>=5G 면 재시작

log() { logger -t kics-reclaim -- "$*" 2>/dev/null || true; echo "kics-reclaim: $*"; }

# kics.service 가 없거나 비활성이면 조용히 종료
if ! systemctl cat "$UNIT" >/dev/null 2>&1; then
    log "unit $UNIT 없음 — skip"; exit 0
fi

# kics 프로세스가 붙잡은 '삭제-오픈' 파일들의 크기 합 (bytes)
held_bytes=$(lsof -nP +L1 2>/dev/null \
    | awk '$1 ~ /kics/ && /\(deleted\)/ { s += $7 } END { printf "%d", s+0 }')
held_gb=$(( held_bytes / 1024 / 1024 / 1024 ))

# 루트 fs 여유 (GB)
free_kb=$(df -P / | awk 'NR==2 { print $4 }')
free_gb=$(( free_kb / 1024 / 1024 ))

log "held(deleted-open)=${held_gb}G, / free=${free_gb}G (threshold=${THRESHOLD_GB}G, min-free=${DISK_MIN_FREE_GB}G)"

reclaim=0
if [ "$held_gb" -ge "$THRESHOLD_GB" ]; then
    reclaim=1
elif [ "$free_gb" -lt "$DISK_MIN_FREE_GB" ] && [ "$held_gb" -ge 5 ]; then
    reclaim=1
fi

if [ "$reclaim" -eq 1 ]; then
    log "회수 실행: $UNIT 재시작 (held=${held_gb}G, free=${free_gb}G)"
    systemctl restart "$UNIT"
    sleep 3
    free_after=$(( $(df -P / | awk 'NR==2 { print $4 }') / 1024 / 1024 ))
    log "완료: / 여유 ${free_gb}G -> ${free_after}G"
else
    log "조치 불필요"
fi
