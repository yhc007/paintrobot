# Paintrobot API 사용법

도장공정 모니터링 시스템 — `paint.coreon.build`

## 시스템 구성

```
[Edge PC] ─HTTPS/JSON──▶ [Cloudflare Tunnel] ──▶ [wasmtime serve]
                                                    │
                                                    ▼
                                                [WASM api-gateway]  ──▶  [CoreDB :9043]
                                                    │                           │
                                                    └──▶ [OpenWeatherMap]       │
                                                                                │
[Browser] ◀──────────── [Cloudflare Tunnel] ──── [React SPA] ◀──────────────────┘
```

- **Base URL**: `https://paint.coreon.build`
- **인증**: 쓰기 엔드포인트는 `X-Edge-Key` 헤더 필수, 읽기는 공개
- **컨텐트 타입**: `application/json` (CSV 제외)
- **시간 형식**: ISO8601 + 오프셋 (`2026-04-27T10:00:00+09:00`)
- **멱등 키**: `event_id` 필드. 같은 값 재전송 시 `duplicates:1` 응답으로 중복 방지

---

## 빠른 참조

| Method | Path | 인증 | 용도 |
|---|---|---|---|
| GET  | `/healthz` | ❌ | 살아있음 확인 |
| POST | `/api/v1/jobs` | ✅ | **카메라/PLC 인식 결과 수신 (실시간)** |
| POST | `/api/v1/jobs/batch` | ✅ | 네트워크 복구 시 일괄 업로드 |
| POST | `/api/v1/coatings` | ✅ | 도막 두께 → 권장 분사압력 계산·저장 |
| GET  | `/api/v1/stats/today` | ❌ | 오늘 모델별 카운트 |
| GET  | `/api/v1/stats/daily?date=` | ❌ | 특정일 통계 |
| GET  | `/api/v1/stats/range?from=&to=&group_by=` | ❌ | 기간 통계 (`day`/`model`) |
| GET  | `/api/v1/jobs?from=&to=&model=&status=&page=` | ❌ | 작업 상세 목록 |
| GET  | `/api/v1/jobs/export.csv?...` | ❌ | CSV 다운로드 |
| GET  | `/api/v1/coatings/today` | ❌ | 오늘 도막 측정 시계열 |
| GET  | `/api/v1/coatings/recent?limit=` | ❌ | 최근 N건 |
| GET  | `/api/v1/weather/current` | ❌ | 현대정밀 위치 온/습도 (OWM) |
| GET  | `/api/v1/stream/live` | ❌ | SSE 실시간 stats 스트림 |

---

## 인증

쓰기(POST) 엔드포인트는 `X-Edge-Key` 헤더가 필요합니다. 키가 잘못되면 HTTP **401**이 떨어집니다.

```http
X-Edge-Key: <엣지 발급 키>
```

> 키는 환경변수 `EDGE_API_KEYS`(콤마 구분)로 서버에 주입됩니다. 키 없이 서버를 띄우면 인증이 비활성화(개발 모드)됩니다.

---

## 1. POST `/api/v1/jobs` — 카메라/PLC 작업 인식 1건

엣지 PC가 PLC 또는 카메라로 모델번호를 인식할 때마다 1건씩 호출합니다.

### 요청 바디

| 필드 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `event_id`        | string | **필수** | 멱등 키 (UUID v7 권장) |
| `edge_id`         | string | **필수** | 엣지 PC 식별자 (예: `edge-line-01`) |
| `plc_model_no`    | string | 선택 | PLC가 알린 모델번호 |
| `camera_model_no` | string | 선택 | 카메라가 인식한 모델번호 (`"1"`~`"8"` 단일 문자도 OK) |
| `plc_ts`          | ISO8601 | 선택 | PLC 신호 시각 |
| `camera_ts`       | ISO8601 | 선택 | 카메라 인식 시각 |
| `confidence`      | number 0.0~1.0 | 선택 | 인식 신뢰도 |
| `image_ref`       | string | 선택 | 캡처 이미지 경로/URL |

> 식별자 허용 문자: 영숫자 + `_ - . :`, 1~128자. 한글/공백 불가.

### `match_status` 자동 분류

- 둘 다 같음 → `matched`
- 둘 다 다름 → `mismatch` (대시보드에 경고로 카운트됨)
- camera만 → `camera_only`
- plc만 → `plc_only`

### 응답 (HTTP 200)

```json
{ "accepted": 1, "duplicates": 0, "rejected": [] }
```

- 같은 `event_id` 재전송 → `{"accepted":0, "duplicates":1, "rejected":[]}`
- 식별자 검증 실패 → `{"accepted":0, "duplicates":0, "rejected":[{"event_id":"...","reason":"..."}]}`

### 예시: 카메라 단독 인식 (제품 "1")

```bash
curl -X POST https://paint.coreon.build/api/v1/jobs \
  -H 'content-type: application/json' \
  -H 'x-edge-key: <YOUR_EDGE_KEY>' \
  -d '{
    "event_id": "01HV0000000000000000000001",
    "edge_id":  "edge-line-01",
    "camera_model_no": "1",
    "camera_ts": "2026-04-27T10:00:00+09:00",
    "confidence": 0.97
  }'
```

### 예시: PLC + 카메라 함께 (불일치 검출)

```bash
curl -X POST https://paint.coreon.build/api/v1/jobs \
  -H 'content-type: application/json' \
  -H 'x-edge-key: <YOUR_EDGE_KEY>' \
  -d '{
    "event_id": "01HV0000000000000000000002",
    "edge_id":  "edge-line-01",
    "plc_model_no":    "HD-A120",
    "camera_model_no": "HD-B200",
    "plc_ts":    "2026-04-27T10:00:00+09:00",
    "camera_ts": "2026-04-27T10:00:02+09:00",
    "confidence": 0.91
  }'
```

### 실시간 송신 (Python, 권장)

```python
import datetime as dt, uuid, requests

URL  = "https://paint.coreon.build/api/v1/jobs"
KEY  = "<YOUR_EDGE_KEY>"
EDGE = "edge-line-01"

S = requests.Session()
S.headers.update({"content-type": "application/json", "x-edge-key": KEY})

def on_camera(model_no: str, confidence: float = 0.95):
    """카메라 추론 콜백에서 즉시 호출"""
    payload = {
        "event_id":        str(uuid.uuid4()),
        "edge_id":         EDGE,
        "camera_model_no": str(model_no),
        "camera_ts":       dt.datetime.now().astimezone().isoformat(timespec="milliseconds"),
        "confidence":      confidence,
    }
    try:
        S.post(URL, json=payload, timeout=3).raise_for_status()
    except requests.RequestException as e:
        # TODO: 로컬 큐에 적재 후 재시도
        print("retry-later:", e, payload["event_id"])

# 카메라가 1~8을 인식할 때마다
on_camera("1")
```

**중요**:
- `Session()`을 재사용해 keep-alive 유지 → 50~150ms 응답
- `event_id`는 매 호출마다 새 값 (UUID v4 또는 v7)
- 네트워크 실패 시 로컬 큐에 적재 후 `/api/v1/jobs/batch`로 일괄 재전송

---

## 2. POST `/api/v1/jobs/batch` — 네트워크 복구 시 일괄 업로드

```json
{
  "edge_id": "edge-line-01",
  "jobs": [
    { "event_id":"...", "edge_id":"edge-line-01", "camera_model_no":"1", ... },
    { "event_id":"...", "edge_id":"edge-line-01", "camera_model_no":"2", ... }
  ]
}
```

응답: `{"accepted":N, "duplicates":M, "rejected":[]}` — 중복은 자동 무시.

---

## 3. POST `/api/v1/coatings` — 도막 두께 → 분사 압력 계산

엣지가 도막 두께 측정값을 보내면, 서버가 OWM 온/습도와 결합해 권장 분사 압력을 계산·반환·저장합니다.

### 요청 바디

| 필드 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `event_id`         | string | **필수** | 멱등 키 |
| `model_no`         | string | **필수** | 측정 대상 모델 |
| `measured_um`      | number | **필수** | 측정 두께 (μm) |
| `current_pressure` | number | **필수** | 현재 분사 압력 (bar) |
| `target_um`        | number | 선택 | 목표 두께. 생략 시 30μm |
| `temperature_c`    | number | 선택 | 생략 시 OWM에서 자동 보충 |
| `humidity_pct`     | number | 선택 | 생략 시 OWM에서 자동 보충 |
| `job_event_id`     | string | 선택 | 연결할 작업의 `event_id` |
| `edge_id`          | string | 선택 | |

### 계산 공식

```
err              = clamp((target_um - measured_um) / target_um, -0.5, +0.5)
control_factor   = 1 + 0.5 × err                 # 두께 미달 → 압력↑
temperature_fac  = 1 - 0.01 × (T_°C  - 20)        # 따뜻할수록 압력↓
humidity_factor  = 1 - 0.003 × (H_%  - 50)        # 습할수록 압력↓
recommended      = clamp(current × 모든 factor 곱, 1.0 bar, 6.0 bar)
```

기준점: 20°C / 50%RH 가 중립 (모든 factor=1.0).

### 응답

```json
{
  "event_id": "co-...",
  "model_no": "HD-A120",
  "measured_um": 24.0,
  "target_um": 30.0,
  "current_pressure": 3.5,
  "recommended_pressure": 4.24,
  "thickness_error": 0.2,
  "temperature_c": 16.35,
  "humidity_pct": 29.0,
  "factors": { "control": 1.1, "temperature": 1.0365, "humidity": 1.063 },
  "measured_at": 1777078517503,
  "work_date": "2026-04-25"
}
```

### 예시

```bash
curl -X POST https://paint.coreon.build/api/v1/coatings \
  -H 'content-type: application/json' \
  -H 'x-edge-key: <YOUR_EDGE_KEY>' \
  -d '{
    "event_id":"co-2026-0001",
    "model_no":"HD-A120",
    "measured_um":24.0,
    "target_um":30.0,
    "current_pressure":3.5
  }'
```

---

## 4. 통계/조회 (인증 불필요)

### `/api/v1/stats/today`
오늘 모델별 카운트 (KST 기준).

```json
{
  "work_date": "2026-04-27",
  "total_jobs": 9,
  "mismatch_jobs": 0,
  "models": [
    {"model_no":"1","job_count":2,"mismatch_count":0},
    {"model_no":"2","job_count":1,"mismatch_count":0}
  ]
}
```

### `/api/v1/stats/daily?date=YYYY-MM-DD`
특정일 통계. 응답 형식은 `today`와 동일.

### `/api/v1/stats/range?from=YYYY-MM-DD&to=YYYY-MM-DD&group_by=day|model`
- `group_by=day`: `DailyStats` 배열 (날짜별)
- `group_by=model`: `[{model_no, job_count, mismatch_count}, ...]` (전 기간 합계)
- 최대 366일

### `/api/v1/jobs?from=&to=&model=&status=&page=&per_page=`
작업 상세 목록 (페이징).

응답:
```json
{
  "total": 123,
  "page": 0,
  "per_page": 200,
  "rows": [
    {
      "event_id":"...","edge_id":"...","plc_model_no":"HD-A120",
      "camera_model_no":"HD-A120","match_status":"matched",
      "plc_ts":1777075200000,"camera_ts":1777075202000,
      "confidence":0.97,"work_date":"2026-04-27"
    }
  ]
}
```

### `/api/v1/jobs/export.csv?...`
같은 필터로 CSV 다운로드.

```
work_date,event_id,edge_id,plc_model_no,camera_model_no,match_status,plc_ts,camera_ts,confidence
2026-04-27,01HV...,edge-line-01,HD-A120,HD-A120,matched,1777075200000,1777075202000,0.97
```

### `/api/v1/coatings/today`, `/api/v1/coatings/recent?limit=N`
도막 측정 시계열. `series` 배열 + 평균값.

---

## 5. GET `/api/v1/weather/current`

현대정밀(`경남 창원시 의창구 반계로 3`, lat 35.2706 / lon 128.6311) 위치의 OWM 실시간 데이터.

```json
{
  "location_name": "현대정밀",
  "lat": 35.2706,
  "lon": 128.6311,
  "observed_at": "2026-04-27T10:00:00+09:00",
  "temperature_c": 17.2,
  "humidity_pct": 58.0,
  "source": "owm"
}
```

OWM 키 미설정 시 `source: "stub"` + 0.0 값으로 응답.

---

## 6. GET `/api/v1/stream/live` — SSE 실시간 스트림

브라우저(또는 EventSource 지원 클라이언트)가 구독하면 **2초마다** 오늘 stats를 푸시합니다. 1시간 후 자동 종료 → 클라이언트 자동 재연결.

```
event: stats
data: {"work_date":"2026-04-27","total_jobs":9,...}

event: stats
data: {"work_date":"2026-04-27","total_jobs":10,...}
```

JS 예:

```js
const es = new EventSource('/api/v1/stream/live');
es.addEventListener('stats', e => {
  const stats = JSON.parse(e.data);
  console.log(stats.total_jobs, stats.models);
});
```

---

## 에러 코드

| 코드 | 의미 | 재시도 |
|---|---|---|
| 200 | 성공 (`accepted`/`duplicates` 둘 다 정상) | — |
| 400 | 페이로드 유효성 실패 (필드 누락, 잘못된 식별자 등) | ❌ 재시도 무의미 |
| 401 | `X-Edge-Key` 누락/오류 | ❌ |
| 404 | 잘못된 경로 | ❌ |
| 502 | 백엔드(CoreDB/OWM) 오류 | ✅ 백오프 후 재시도 |
| 503 | OWM 키 누락 | ⚠️ 키 설정 필요 |

응답 본문 예 (실패):
```json
{ "error": "missing or invalid X-Edge-Key" }
```

---

## 운영 정보

### 시스템 stack
- **WASM 런타임**: Wasmtime serve (`wasi:http/proxy`)
- **빌드 타깃**: `wasm32-wasip2`
- **DB**: CoreDB on port 9043 (HTTP API, 단일 노드, append-only)
- **터널**: cloudflared `paintrobot` 터널, 4 connection (icn01/05/06)
- **프론트**: Vite + React, 정적 서빙 :5174

### systemd user services (linger 활성)

| 유닛 | 포트 | 역할 |
|---|---|---|
| `paintrobot-api`    | 18080 | wasmtime serve → WASM 컴포넌트 |
| `paintrobot-web`    | 5174  | Vite 빌드 결과 정적 서빙 |
| `paintrobot-tunnel` | —     | cloudflared `paintrobot` 터널 |
| `coredb` (시스템)    | 9043  | CoreDB HTTP API |

### 운영 명령

```bash
# 재시작
systemctl --user restart paintrobot-api
systemctl --user restart paintrobot-tunnel
systemctl --user restart paintrobot-web

# 로그 확인
journalctl --user -u paintrobot-api -f
tail -f /home/root1/Work/Paintrobot/logs/wasmtime.log

# WASM 재빌드 후 재기동
cargo build --target wasm32-wasip2 -p paintrobot-api-gateway --release
systemctl --user restart paintrobot-api

# 프론트엔드 재빌드 후 배포
cd web && npm run build && systemctl --user restart paintrobot-web
```

### 환경변수 (`/home/root1/Work/Paintrobot/.env.local`)

```
OWM_API_KEY=...
EDGE_API_KEYS=key1,key2,key3
```

권한: `chmod 600 .env.local`. 절대 git 커밋 금지 (`.gitignore`에 등록됨).

---

## CoreDB 스키마 (참고)

CoreDB는 CQL의 매우 제한된 서브셋만 지원합니다. 단일 PRIMARY KEY, UPDATE/DELETE 미지원, counter 미지원.

```cql
CREATE KEYSPACE paintrobot WITH REPLICATION = {'class':'SimpleStrategy','replication_factor':1};

CREATE TABLE paintrobot.jobs (
  event_id TEXT PRIMARY KEY,
  edge_id TEXT, plc_model_no TEXT, camera_model_no TEXT,
  plc_ts BIGINT, camera_ts BIGINT, confidence DOUBLE,
  match_status TEXT, work_date TEXT, image_ref TEXT, created_at BIGINT
);

CREATE TABLE paintrobot.weather_snapshots (
  observed_at TEXT PRIMARY KEY,
  temperature_c DOUBLE, humidity_pct DOUBLE, source TEXT
);

CREATE TABLE paintrobot.coatings (
  event_id TEXT PRIMARY KEY, job_event_id TEXT, model_no TEXT,
  measured_um DOUBLE, target_um DOUBLE,
  temperature_c DOUBLE, humidity_pct DOUBLE,
  current_pressure DOUBLE, recommended_pressure DOUBLE,
  thickness_error DOUBLE,
  control_factor DOUBLE, temp_factor DOUBLE, humidity_factor DOUBLE,
  measured_at BIGINT, work_date TEXT
);
```

집계는 서버에서 SELECT + 인메모리 그룹핑으로 계산 (UPDATE/counter 없음).

---

## 참고 링크

- 대시보드: <https://paint.coreon.build>
- GitHub: <https://github.com/yhc007/paintrobot>
- CoreDB: <https://github.com/yhc007/coredb>
