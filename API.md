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
| POST | `/api/v1/plc/recipe` | ✅ | **차종 도장 레시피 수신 (멱등 upsert)** |
| GET  | `/api/v1/plc/recipe/current` | ❌ | 오늘 마지막으로 수신한 레시피 (`?edge_id=` 필터) |
| POST | `/api/v1/coatings` | ✅ | 도막 두께 → 권장 분사압력 계산·저장 |
| GET  | `/api/v1/stats/today` | ❌ | 오늘 모델별 카운트 |
| GET  | `/api/v1/stats/daily?date=` | ❌ | 특정일 통계 |
| GET  | `/api/v1/stats/range?from=&to=&group_by=` | ❌ | 기간 통계 (`day`/`model`) |
| GET  | `/api/v1/stats/bounds` | ❌ | 집계 데이터가 존재하는 최초/최종 날짜 |
| GET  | `/api/v1/stats/mixflow?date=` | ❌ | 혼류 생산 지표 (전환·연속·단독 투입·순서) |
| GET  | `/api/v1/stats/reconcile?date=` | ❌ | PLC↔카메라 지연 상관 **추정** (읽기 전용) |
| POST | `/api/v1/jobs/reconcile?date=` | ✅ | 상관 결과를 `match_status`에 기록 (기본 dry-run) |
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

## 3-1. POST `/api/v1/plc/recipe` — 차종 도장 레시피 수신

엣지 리더(`hdm_paint`)가 PLC에서 읽은 **현재 차종의 도장 레시피**를 전송합니다.
`(edge_id, model_no)` 당 **1행으로 멱등 upsert** — 폴링(`--watch N`)으로 반복 전송해도
행이 증가하지 않고 최신값으로 갱신됩니다.

### 요청 바디

| 필드 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `edge_id`   | string  | **필수** | 엣지 식별자 (예: `edge-line-01`) |
| `model_no`  | integer | **필수** | 차종번호 = HMI 선택값 (1~8). **정수** (jobs의 문자열 model_no와 다름) |
| `model_name`| string  | **필수** | 모델명 (예: `"140"`) |
| `levels`    | integer | **필수** | 각 파라미터 배열의 구간 수 (예: 8) |
| `recipe.atomization` | `{table:[int], applied:[int]}` | **필수** | 무화 (저장값/적용값) |
| `recipe.pattern`     | `{table:[int], applied:[int]}` | **필수** | 패턴 |
| `recipe.flow`        | `{table:[int], applied:[int]}` | **필수** | 토출량 |

- `table`/`applied` 배열 길이는 **반드시 `levels` 와 동일** (아니면 400).
- 값은 INT16 범위. `0`도 정상 데이터로 저장 (스프레이 미분사 시 0으로 내려감).
- `table` = PLC 마스터 저장값, `applied` = 현재 적용값 (캘리브레이션으로 다를 수 있음).

### 응답 (HTTP 200)

```json
{ "result": "ok", "event_id": "recipe-edge-line-01-8", "model_no": 8, "model_name": "140" }
```

- `401` 인증 실패 / `400` 스키마·배열 길이 오류 (`{"error":"...설명..."}`) / `502` CoreDB 오류.

### 예시

```bash
curl -X POST http://192.168.10.30:18080/api/v1/plc/recipe \
  -H "Content-Type: application/json" \
  -H "x-edge-key: <YOUR_EDGE_KEY>" \
  -d '{"edge_id":"edge-line-01","model_no":8,"model_name":"140","levels":8,"recipe":{"atomization":{"table":[50,50,50,50,50,0,0,0],"applied":[35,35,35,35,0,0,0,0]},"pattern":{"table":[30,30,30,30,30,0,0,0],"applied":[20,20,20,20,0,0,0,0]},"flow":{"table":[0,0,0,0,0,0,0,0],"applied":[0,0,0,0,0,0,0,0]}}}'
```

### GET `/api/v1/plc/recipe/current` — 최신 레시피 조회 (인증 불필요)

오늘 수신한 레시피 중 가장 최근 것을 반환. `?edge_id=edge-line-01` 로 라인 필터.
저장된 `recipe` 오브젝트를 그대로 되돌려줍니다. 없으면 `{"model_no":null,"recipe":null}`.

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

### `/api/v1/stats/bounds`
집계에 잡히는 작업이 실제로 존재하는 **최초/최종 `work_date`**.

대시보드가 조회 구간에서 빈 결과를 받았을 때 호출해, 빈 차트를 보여주는 대신
데이터가 있는 구간으로 범위를 옮기는 용도입니다.

응답:
```json
{ "first_date": "2026-04-24", "last_date": "2026-08-27" }
```

- `match_status`가 `plc_only`인 행은 제외됩니다. 대시보드 집계에서도 빠지는 값이라,
  PLC 신호만 있는 날은 "데이터가 있는 날"로 안내하면 안 되기 때문입니다.
- 집계 대상이 하나도 없으면 두 필드 모두 `null`:
  ```json
  { "first_date": null, "last_date": null }
  ```

### `/api/v1/stats/mixflow?date=YYYY-MM-DD`
투입 **순서**에서만 나오는 값들. 일자별 합계로는 혼류가 보이지 않는다 — 같은
100대라도 한 차종을 몰아서 만든 것과 여러 차종이 섞여 흐른 것은 라인에 전혀
다른 부담인데, 합계를 내면 그 차이가 사라진다.

```json
{
  "work_date":"2026-08-24", "units":123, "models":7,
  "changeovers":17, "changeover_rate":0.139,
  "avg_run":6.83, "max_run":26, "singles":6,
  "runs":[{"model_no":"1","count":1,"start_ms":1787...}, ...]
}
```

- `changeover_rate` — 전환 / (대수-1). 1에 가까울수록 매 대마다 차종이 바뀐다
- `singles` — 1대만 끼어든 투입. 차 한 대 지나가는 동안 레시피를 바꿔야 하는,
  혼류에서 가장 어려운 케이스
- `runs` — 연속 구간 목록. 대수 단위가 아니라 구간 단위라 응답이 작다
- 생산으로 세는 기준은 `stats/daily`와 같다 (`plc_only` 제외)

### `/api/v1/stats/reconcile?date=YYYY-MM-DD`
PLC 상태와 카메라 인식을 **사후에 이어붙인 추정치**. DB는 건드리지 않는다.

왜 사후인가: 카메라가 PLC보다 앞선다. 투입구에서 읽힌 차가 도장 부스에
도착해야 PLC 상태에 반영되므로, 카메라 이벤트가 들어오는 순간에는 짝이 될
PLC 지시가 아직 없다. 그래서 ingest 시점 실시간 상관이 불가능하다.

```json
{
  "work_date":"2026-08-27", "offset_secs":360,
  "plc_states":7, "camera_events":73,
  "matched":64, "mismatch":7,
  "skipped_low_confidence":2, "skipped_no_plc":0,
  "dry_run":true, "written":0
}
```

- `offset_secs` — 데이터에서 추정한 지연. 상수가 아니다 (관측 6~9.5분).
  표본이 10대 미만이거나 PLC 상태 전환이 2회 미만이면 `null` (추정 거부).
- `skipped_low_confidence` — 카메라 신뢰도가 `min_confidence`(기본 0.7) 미만.
  못 믿는 판독으로 "불일치"를 만들지 않는다.
- `after_changeover` — 전환 직후 구간별 정합/불일치. 구간을 **카메라 쪽 런
  위치**(첫 대 / 2~3대째 / 4대째 이후)로 나눈다. PLC 전환으로부터의 거리로
  나누면 추정 오프셋의 오차가 그대로 섞이지만, 런 위치는 카메라 순서만으로
  정해져 오프셋과 무관하다. `mismatch_rate`는 표본이 없으면 `null` —
  0으로 내려보내면 "이상 없음"으로 읽힌다.
- ⚠️ 전체 불일치를 그대로 품질 지표로 쓰지 말 것. 정합 자체가 추정치이고,
  전환 직후 값이 높은 것이 부스가 레시피를 못 따라간 탓인지 이송 지연 추정의
  오차인지는 아직 구분되지 않는다.

### `POST /api/v1/jobs/reconcile?date=&dry_run=&min_confidence=`
같은 계산을 하고 결과를 `match_status`에 **기록**한다. `X-Edge-Key` 필요.

- `dry_run` 기본 **true** — 실제로 쓰려면 `dry_run=false` 명시
- 대상은 `plc_ts`가 없는 행. 이 배치가 이미 쓴 행도 다시 집으므로 재실행 가능
- 엣지가 직접 짝지어 보낸 행(`plc_ts` 있음)은 건드리지 않는다

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
