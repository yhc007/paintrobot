# kics-reclaim

Kaspersky KICS(`kics.service`)가 삭제-오픈 임시파일로 붙잡은 디스크를 주기적으로 회수한다.

## 배경
KICS 스캔은 `/tmp/PR*.tmp` 같은 대용량 임시파일을 만든 뒤 `unlink` 하지만 파일핸들을
닫지 않는다. 그래서 파일은 이름상 사라졌지만(`(deleted)`) 디스크 블록은 계속 점유되어
`df` 사용량이 오르고, 실측에서 **183GB**까지 붙잡아 루트 fs를 100%로 채운 사례가 있었다.
`rm`/`truncate` 로는 못 없앤다 — 프로세스가 핸들을 닫아야 회수되므로 `kics.service` 재시작이
유일한 방법이다.

## 동작
- `kics-reclaim.timer` 가 부팅 5분 후 + 이후 10분마다 `kics-reclaim.service`(oneshot) 실행.
- 스크립트가 `lsof +L1` 로 kics 프로세스가 쥔 삭제-오픈 파일 총량을 잰다.
- 총량이 임계치(기본 20GB) 이상이거나, 루트 여유가 25GB 미만이면서 5GB 이상 붙잡고 있으면
  `systemctl restart kics.service` 로 핸들을 닫아 공간을 회수한다.
- 그 외에는 아무것도 하지 않는다(불필요한 보안SW 재시작 방지). 로그는 journal(`-t kics-reclaim`).

## 튜닝 (환경변수, service 에 `Environment=` 로 주입)
- `KICS_RECLAIM_THRESHOLD_GB` (기본 20)
- `KICS_RECLAIM_MIN_FREE_GB` (기본 25)
- `KICS_UNIT` (기본 `kics.service`)

## 설치
```bash
sudo install -m0755 kics-reclaim.sh /usr/local/bin/kics-reclaim.sh
sudo install -m0644 kics-reclaim.service /etc/systemd/system/kics-reclaim.service
sudo install -m0644 kics-reclaim.timer   /etc/systemd/system/kics-reclaim.timer
sudo systemctl daemon-reload
sudo systemctl enable --now kics-reclaim.timer
```

## 확인 / 운영
```bash
systemctl status kics-reclaim.timer          # 타이머 상태
systemctl list-timers kics-reclaim.timer     # 다음 실행 시각
sudo systemctl start kics-reclaim.service    # 즉시 1회 실행
journalctl -t kics-reclaim -n 30 --no-pager  # 로그
```

## 근본 해결
이 루틴은 증상 완화용이다. KICS 콘솔에서 스캔 임시경로/범위 설정을 점검해 임시파일이
과도하게 쌓이지 않게 하는 것이 정석이다.
