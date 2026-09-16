# 엣지 → 서버 : R1 로봇(MPX2600) 인터페이스 전송 API 스펙

현대정밀 R1 도장라인 엣지 리더(`hdm_paint --post-robot`)가 PLC에서 읽은
**로봇 인터페이스 상태(지령/상태 비트 + JIG 시프트 WORK ID)** 를 서버로 POST 전송하기 위한
연동 규격입니다. **서버 팀은 아래 스펙대로 수신 라우트를 구현**하면 됩니다.

> 기존 `POST /api/v1/plc/model`(차종번호), `POST /api/v1/plc/recipe`(도장 레시피)와
> 형제 관계인 **세 번째 엔드포인트**입니다. 인증 방식·엣지 식별자 규칙은 동일합니다.
> 레시피 스펙은 [`API_레시피_전송_스펙.md`](API_레시피_전송_스펙.md) 참조.

---

## 0. 배경 — 이 데이터가 왜 따로 필요한가

PLC는 도장 파라미터(토출량/무화/패턴)를 **로봇에 넘기지 않습니다.**
PLC가 D/A 출력으로 도장건을 직접 제어하고, 로봇에는 **"어떤 차종인가"(WORK ID)** 와
**기동/모드 지령 비트**만 접점으로 전달합니다. 로봇은 그 WORK ID로 내부에 티칭된
차종별 JOB을 스스로 골라 궤적을 실행합니다.

```
HMI 차종번호(%DW5000)
   ├─→ 레시피 로드 → D/A 출력 → 도장건       ← /api/v1/plc/recipe 로 전송
   └─→ JIG 시프트 WORK ID(%DW7200~7240)
          └─→ M4800~M4864(로봇전송) + %PX332(WORK ID) + %MX2417(START)
                 └─→ 로봇: 차종별 JOB 실행     ← /api/v1/plc/robot 로 전송  ★이 문서
```

따라서 이 엔드포인트가 답하는 질문은 **"몇 번 차체가, 어느 JIG 위치에서, 로봇에
언제 넘어갔고, 로봇이 그걸 받아서 돌고 있는가"** 입니다. 도장 수치가 아니라
**핸드셰이크와 추적(traceability)** 데이터입니다.

---

## 1. 엔드포인트 요약

| 항목 | 값 |
|---|---|
| Method | `POST` |
| URL | `http://192.168.10.30:18080/api/v1/plc/robot` |
| Content-Type | `application/json` |
| 인증 헤더 | `x-edge-key: <엣지 키>` (model/recipe 엔드포인트와 동일) |
| 인코딩 | UTF-8, JSON |
| 전송 주기 | 1회성 또는 폴링(`--watch N`초). **상태 스냅샷**이므로 시계열 append 권장 |

**요청 헤더 예시**
```
POST /api/v1/plc/robot HTTP/1.1
Host: 192.168.10.30:18080
Content-Type: application/json
x-edge-key: 90K20HlsV3BstN-AVvkUCij0da5M4Kl6
Content-Length: <n>
Connection: close
```

---

## 2. 요청 본문(JSON) 스키마

```jsonc
{
  "edge_id":     "string",        // 엣지 식별자 (예: "edge-line-01")
  "robot_model": "MPX2600",       // 고정 문자열 (로봇 기종)
  "model_no":    integer | null,  // 차종번호 %DW5000 (1~8). 대조용

  "robot": {
    "command": { <bool|null> },   // PLC → 로봇 지령   (내부 릴레이 M)
    "state":   { <bool|null> },   // 로봇 운전 상태     (내부 릴레이 M)
    "alarm":   { <bool|null> },   // 로봇 알람          (내부 릴레이 M)
    "do":      { <bool|null> },   // PLC → 로봇 출력접점 (P 영역, 실제 배선)
    "di":      { <bool|null> }    // 로봇 → PLC 입력접점 (P 영역, 실제 배선)
  },

  "jig": {
    "shift_distance":       integer | null,
    "chattering_guard":     integer | null,
    "start_sig_start_dist": integer | null,
    "start_sig_end_dist":   integer | null,
    "common":   { <bool|null> },  // 시프트 공통 비트
    "stations": [ { ... } ],      // 항상 5개 (JIG 1~5), no 오름차순
    "completed": { "work_id": integer|null, "work_in": integer|null }
  },

  "read_errors": [ "string", ... ] // 비었으면 전 구간 정상. §6 참조
}
```

`jig.stations[]` 원소:

```jsonc
{
  "no":             1,               // 1~5 고정
  "work_id":        integer | null,  // 이 JIG에 실린 차체의 차종번호
  "work_in":        integer | null,  // 0 = 워크 없음, 그 외 = 있음
  "shift_dist":     integer | null,  // 컨베이어 시프트 거리값
  "job_start_dist": integer | null,  // 로봇 JOB 시작 거리 임계값
  "send_to_robot":  bool | null,     // 이 JIG 정보를 로봇으로 전송 중
  "job_start":      bool | null,     // 로봇 JOB START 발행
  "send_done":      bool | null      // 전송 종료
}
```

### 값 규칙

- **모든 비트 필드는 `true` / `false` / `null` 3-state.** `null` = 해당 워드 블록
  읽기 실패(= 값 모름)이며, `false`와 **반드시 구분해서** 저장해야 합니다.
  비상정지 비트가 `null`인데 `false`로 저장되면 "안전함"으로 오독됩니다.
- **모든 정수 필드는 INT16(부호 있음, -32768~32767) 또는 `null`.**
- `robot.command` / `robot.do` 의 키 일부는 이름이 겹칩니다
  (`external_start`, `servo_on` 등). **전자는 PLC 내부 릴레이, 후자는 실제 출력 접점**으로
  의미가 다르므로 **반드시 그룹 단위로 분리 저장**하세요. 둘이 어긋나면 배선/출력 모듈 이상 신호입니다.
- 키 집합은 고정입니다. 서버는 **모르는 키가 추가돼도 무시**하도록(forward-compatible) 구현하세요.

---

## 3. 필드 상세 & PLC 소스 매핑

리더가 각 필드를 어느 PLC 주소에서 읽는지 (추적/검증용).
주소는 `docs/symbols.csv` 표기(= 선형 비트 인덱스, `%MX`/`%PX`)이며,
XG5000 래더의 XGK 표기로는 `워드 = 인덱스/16`, `비트 = 인덱스%16` 입니다.
(예: `M12000` → 워드 750, 비트 0 → 래더상 `M07500`)

### 3.1 `robot.command` — PLC → 로봇 지령 (내부 릴레이)

| JSON 키 | PLC | 의미 |
|---|---|---|
| `play_mode_select` | `%MX2401` | MPX2600 PLAY MODE SELECT |
| `call_master_job` | `%MX2402` | MPX2600 CALL MASTER JOB |
| `servo_on` | `%MX2403` | MPX2600 SERVO ON |
| `teach_mode_select` | `%MX2404` | MPX2600 TEACH MODE SELECT |
| `auto_ready_ok` | `%MX2416` | MPX2600 AUTO READY OK |
| `external_start` | `%MX2417` | MPX2600 EXTERNAL START |

### 3.2 `robot.state` — 로봇 운전 상태

| JSON 키 | PLC | 의미 |
|---|---|---|
| `auto_running` | `%MX2405` | MPX2600 자동운전중 |
| `ready_ok` | `%MX2406` | READY O.K |
| `auto_stop` | `%MX2407` | 자동 정지 |
| `auto_run_condition` | `%MX12000` | 로봇 자동운전 조건만족 (래더 `M07500`) |
| `auto_run_started` | `%MX12016` | 자동운전 시작 (래더 `M07510`) |

### 3.3 `robot.alarm` — 로봇 알람

| JSON 키 | PLC | 의미 |
|---|---|---|
| `panel_estop` | `%MX16016` | MPX2600 PANEL 비상정지 |
| `pendant_estop` | `%MX16017` | MPX2600 PANDENT 비상정지 |
| `fault` | `%MX16018` | MPX2600 이상 |
| `battery_fault` | `%MX16019` | MPX2600 BATTERY 이상 |

### 3.4 `robot.do` — PLC → 로봇 출력 접점 (P 영역)

| JSON 키 | PLC | 의미 |
|---|---|---|
| **`work_id_1`** | **`%PX332`** | **MPX2600 WORK ID. 1 — 차종번호 전달 비트** |
| `external_start` | `%PX320` | MPX2600 EXTERNAL START |
| `call_master_job` | `%PX322` | MPX2600 CALL MASTER JOB |
| `alarm_reset` | `%PX323` | MPX2600 ALARM/ERROR RESET |
| `servo_on` | `%PX324` | MPX2600 EXTERNAL SERVO ON |
| `play_mode_select` | `%PX325` | MPX2600 PLAY MODE SELECT |
| `teach_mode_select` | `%PX326` | MPX2600 TEACH MODE SELECT |
| `external_hold` | `%PX327` | MPX2600 EXTERNAL HOLD |
| `job_start` | `%PX340` | MPX2600 IN09- JOB START |
| `move_clean_pos` | `%PX342` | MPX2600 IN11- 세정위치 이동 |
| `move_home_pos_1` | `%PX343` | MPX2600 IN12- 홈위치 이동 |
| `move_home_pos_2` | `%PX344` | MPX2600 IN13- 홈위치 이동 |
| `cycle_stop` | `%PX345` | MPX2600 IN14- CYCLE STOP |
| `safety_plug` | `%PX348` | MPX2600 EXTERNAL SAFETY PLUG |
| `emergency_stop` | `%PX349` | EXTERNAL EMERGENCY STOP |
| `conveyor_st12` | `%PX350` | ROBOT C/V ST12 |

### 3.5 `robot.di` — 로봇 → PLC 입력 접점 (P 영역)

| JSON 키 | PLC | 의미 |
|---|---|---|
| `running` | `%PX192` | MPX2600 RUNNING |
| `servo_on` | `%PX193` | MPX2600 SERVO ON |
| `top_of_master_job` | `%PX194` | MPX2600 TOP OF MASTER JOB |
| `fault` | `%PX195` | MPX2600 이상 |
| `battery_fault` | `%PX196` | MPX2600 BATTERY 이상 |
| `remote_mode_selected` | `%PX197` | MPX2600 REMOTE MODE SELECTED |
| `play_mode_selected` | `%PX198` | MPX2600 PLAY MODE SELECTED |
| `teach_mode_selected` | `%PX199` | MPX2600 TEACH MODE SELECTED |
| `home_position` | `%PX202` | MPX2600 HOME POSITION |
| `start_permit` | `%PX203` | MPX2600 START PERMIT |
| `spray_ch1` | `%PX204` | MPX2600 SPRAY CH1 (분사 채널) |
| `cp_estop` | `%PX206` | MPX2600 C.P 비상정지 |
| `pp_estop` | `%PX207` | MPX2600 P.P 비상정지 |
| `job_in_progress` | `%PX208` | MPX2600 0T01- JOB IN PROGRESS |
| `at_clean_position` | `%PX211` | MPX2600 OT04- 세정위치 도달 |
| `level` | `%PX212` | MPX2600 0T05- 레벨 |

### 3.6 `jig` — 컨베이어 JIG 시프트 레지스터 (5단)

| JSON 경로 | PLC | 의미 |
|---|---|---|
| `jig.shift_distance` | `%DW7000` | JIG SHIFT 거리 |
| `jig.chattering_guard` | `%DW7010` | JIG SHIFT 체터링 방지거리 |
| `jig.start_sig_start_dist` | `%DW7020` | JIG 1 SHFT END ROBOT START SIG. START DIST |
| `jig.start_sig_end_dist` | `%DW7025` | JIG 1 SHFT END ROBOT START SIG. END DIST |
| `jig.common.data_shift_on` | `%MX4112` | DATA SHIFT ON |
| `jig.common.work_data_reset` | `%MX4113` | WORK DATA RESET |
| `jig.common.robot_cv_start` | `%MX4960` | ROBOT CV START |
| `jig.common.robot_cv_start_2` | `%MX4961` | ROBOT CV START (2) |
| `jig.common.robot_job_start` | `%MX4992` | ROBOT JOB START |
| `jig.common.work_in_not_on` | `%MX4993` | WORK IN/NOT ON |
| `jig.common.robot_job_start_2` | `%MX4994` | ROBOT JOB START (2) |
| `jig.completed.work_id` | `%DW7500` | JIG SHFT 완료 WORK ID |
| `jig.completed.work_in` | `%DW7502` | WORK IN/NOT |

`jig.stations[n]` (n = 0..4, `no` = n+1):

| JSON 키 | PLC 주소식 | JIG1 | JIG5 |
|---|---|---|---|
| `work_id` | `%DW7200 + 10n` | `%DW7200` | `%DW7240` |
| `work_in` | `%DW7205 + 10n` | `%DW7205` | `%DW7245` |
| `shift_dist` | `%DW7100 + 10n` | `%DW7100` | `%DW7140` |
| `job_start_dist` | `%DW7105 + 10n` | `%DW7105` | `%DW7145` |
| `send_to_robot` | `%MX4800 + 16n` | `%MX4800` | `%MX4864` |
| `job_start` | `%MX4801 + 16n` | `%MX4801` | `%MX4865` |
| `send_done` | `%MX4802 + 16n` | `%MX4802` | `%MX4866` |

---

## 4. 서버에서 받아 처리할 부분

### 4.1 저장 모델 (권장)

레시피와 달리 이 데이터는 **순간 상태 스냅샷**입니다. upsert로 덮어쓰면 핸드셰이크
이력이 사라지므로 **시계열 append** 를 권장합니다.

| 테이블 | 내용 | 키 |
|---|---|---|
| `plc_robot_snapshot` | 수신 원본 1건 = 1행 (JSON 그대로 + 수신시각) | `(edge_id, received_at)` |
| `plc_robot_jig_state` | `stations[]` 전개 5행 | `(edge_id, received_at, jig_no)` |
| `plc_robot_event` | 아래 §4.3 파생 이벤트 | `(edge_id, occurred_at, event)` |

- **타임스탬프는 서버가 찍습니다.** 엣지 PLC에는 신뢰할 시계가 없어 페이로드에
  시각 필드를 넣지 않았습니다. 수신 시각(`received_at`)을 서버에서 부여하세요.
- 폴링 주기가 짧으면(1~2초) 행이 빠르게 쌓입니다. 원본 스냅샷은 보존기간(예: 30일)
  롤오프, 파생 이벤트는 장기 보존을 권장합니다.

### 4.2 검증 (400으로 거절할 것)

| 항목 | 규칙 |
|---|---|
| `edge_id` | 필수, 비어있지 않은 문자열 |
| `jig.stations` | 필수, 길이 정확히 5, `no` = 1..5 각 1회 |
| 비트 필드 | `true`/`false`/`null` 외의 값 거절 (문자열 `"true"` 등 금지) |
| 정수 필드 | INT16 범위 밖이면 거절 |
| `model_no` | `null` 허용. 값이 있으면 1~8 밖이어도 **거절하지 말고 저장 + 경고 플래그**<br/>(HMI 미입력/전환 중에 0이나 범위 밖 값이 실제로 나옴) |

### 4.3 파생 이벤트 (모니터링에 실제로 쓰이는 것)

스냅샷을 그대로 쌓는 것만으로는 쓸모가 적습니다. **직전 스냅샷과 비교한 edge 검출**을
서버에서 하는 것을 권장합니다.

| 이벤트 | 검출 조건 | 의미 |
|---|---|---|
| `jig_handoff` | `stations[n].send_to_robot` false→true | n번 JIG 차체 정보가 로봇으로 넘어감 |
| `jig_handoff_done` | `stations[n].send_done` false→true | 전송 완료 |
| `robot_job_started` | `robot.di.job_in_progress` false→true | 로봇이 JOB 착수 |
| `robot_job_ended` | `robot.di.job_in_progress` true→false | 로봇 JOB 종료 (= 1사이클) |
| `cycle_completed` | `jig.completed.work_id` 값 변화 | 도장 1사이클 완료, 값 = 차종번호 |
| `robot_fault` | `robot.alarm.*` 또는 `robot.di.fault` false→true | 알람 발생 |
| `estop` | `*_estop` 계열 false→true | 비상정지 |
| **`work_id_mismatch`** | `model_no` ≠ 활성 JIG의 `work_id` | **HMI 입력과 실제 차체 불일치 — 오도장 위험** |

> `work_id_mismatch` 가 이 엔드포인트의 핵심 가치입니다. HMI에서 작업자가 차종을
> 바꿨는데 컨베이어 위 차체는 이전 차종인 구간이 실제로 생기며, 그 구간에 도장되면
> 레시피가 어긋납니다. `model_no`(현재 HMI 값)와 `stations[].work_id`(실제 차체에
> 붙어 시프트되는 값)를 비교하면 그 구간을 잡아낼 수 있습니다.

### 4.4 `robot.command` vs `robot.do` 불일치 감시

같은 이름의 쌍(`external_start`, `servo_on`, `play_mode_select`, `teach_mode_select`,
`call_master_job`)은 정상 상태에서 **같은 값**이어야 합니다. 내부 릴레이는 ON인데
출력 접점이 OFF면 출력 모듈/배선 고장 의심 → 별도 경보 대상으로 두세요.
(스캔 타이밍 차이로 1스냅샷 정도 어긋나는 것은 정상이므로, **연속 N회 불일치**일 때만 경보)

---

## 5. 실제 페이로드 예시

차종번호 8이 JIG 2에 실려 로봇으로 넘어가고, 로봇이 JOB 수행 중인 상태:

```json
{
  "edge_id": "edge-line-01",
  "robot_model": "MPX2600",
  "model_no": 8,
  "robot": {
    "command": {
      "play_mode_select": true, "call_master_job": false, "servo_on": true,
      "teach_mode_select": false, "auto_ready_ok": true, "external_start": false
    },
    "state": {
      "auto_running": true, "ready_ok": true, "auto_stop": false,
      "auto_run_condition": true, "auto_run_started": true
    },
    "alarm": {
      "panel_estop": false, "pendant_estop": false,
      "fault": false, "battery_fault": false
    },
    "do": {
      "work_id_1": true, "external_start": false, "call_master_job": false,
      "alarm_reset": false, "servo_on": true, "play_mode_select": true,
      "teach_mode_select": false, "external_hold": false, "job_start": true,
      "move_clean_pos": false, "move_home_pos_1": false, "move_home_pos_2": false,
      "cycle_stop": false, "safety_plug": true, "emergency_stop": false,
      "conveyor_st12": true
    },
    "di": {
      "running": true, "servo_on": true, "top_of_master_job": false,
      "fault": false, "battery_fault": false, "remote_mode_selected": true,
      "play_mode_selected": true, "teach_mode_selected": false,
      "home_position": false, "start_permit": true, "spray_ch1": true,
      "cp_estop": false, "pp_estop": false, "job_in_progress": true,
      "at_clean_position": false, "level": false
    }
  },
  "jig": {
    "shift_distance": 1200,
    "chattering_guard": 30,
    "start_sig_start_dist": 150,
    "start_sig_end_dist": 900,
    "common": {
      "data_shift_on": false, "work_data_reset": false,
      "robot_cv_start": true, "robot_cv_start_2": true,
      "robot_job_start": true, "work_in_not_on": true, "robot_job_start_2": true
    },
    "stations": [
      {"no":1,"work_id":8,"work_in":1,"shift_dist":1200,"job_start_dist":150,
       "send_to_robot":false,"job_start":false,"send_done":true},
      {"no":2,"work_id":8,"work_in":1,"shift_dist":980,"job_start_dist":150,
       "send_to_robot":true,"job_start":true,"send_done":false},
      {"no":3,"work_id":0,"work_in":0,"shift_dist":0,"job_start_dist":150,
       "send_to_robot":false,"job_start":false,"send_done":false},
      {"no":4,"work_id":0,"work_in":0,"shift_dist":0,"job_start_dist":150,
       "send_to_robot":false,"job_start":false,"send_done":false},
      {"no":5,"work_id":0,"work_in":0,"shift_dist":0,"job_start_dist":150,
       "send_to_robot":false,"job_start":false,"send_done":false}
    ],
    "completed": {"work_id": 7, "work_in": 1}
  },
  "read_errors": []
}
```

> 위 값은 **스키마 예시**입니다. 실제 라인 값은 `--dry-run`으로 캡처해 확인하세요.

---

## 6. `read_errors` — 부분 실패 처리

리더는 5개의 워드 블록을 각각 읽습니다. 블록 하나가 실패해도 사이클을 중단하지 않고,
**그 블록에서 나오는 필드만 `null`** 로 채운 뒤 실패 내역을 `read_errors`에 넣어 전송합니다.

| 블록 | 영향받는 필드 |
|---|---|
| `%MW150 x163` | `robot.command`, `robot.state`(일부), `jig.common`, `stations[].send_*`/`job_start` |
| `%MW750 x252` | `robot.state.auto_run_*`, `robot.alarm` |
| `%PW12 x10` | `robot.do`, `robot.di` 전부 |
| `%DW7000 x246` | `jig.shift_distance`~`start_sig_*`, `stations[].work_id`/`work_in`/`*_dist` |
| `%DW7500 x3` | `jig.completed` |

서버 처리 지침:

1. `read_errors`가 비어있지 않으면 **데이터 품질 플래그를 세워 저장**하고, 그 스냅샷은
   §4.3의 edge 검출에서 **제외**하세요. (null→false 전이를 가짜 이벤트로 잡지 않도록)
2. 같은 블록 실패가 **연속 N회**(예: 5회) 이어지면 "엣지-PLC 통신 이상"으로 경보.
3. 문자열은 `%<영역>W<시작> x<워드수>words: <원인>` 형식이며 ASCII만 들어갑니다
   (Windows 소켓 에러 메시지의 비ASCII 구간은 `?`로 축약). 예:
   `"%PW12 x10words: connect 192.168.0.10:2004 ?: ? ?. (os error 10060)"`
4. `read_errors`가 5건 = 전 블록 실패 = PLC 연결 자체가 끊긴 상태입니다.

---

## 7. 응답 규격 (서버가 반환해야 할 것)

| 상황 | HTTP | 본문(예) |
|---|---|---|
| 정상 수신/저장 | `200` (또는 `201`) | `{"result":"ok"}` (형식 자유) |
| 인증 실패 | `401` | `{"error":"invalid edge key"}` |
| 스키마 오류 | `400` | `{"error":"...설명..."}` |
| 서버 내부 오류 | `5xx` | 자유 |

- 엣지 리더는 **2xx = 성공**으로 판단하고, 그 외에는 경고 로그만 남깁니다.
  - one-shot 모드: 비2xx 시 응답 본문을 stderr에 출력.
  - watch 모드: 실패해도 다음 주기 계속 진행 (**재전송/버퍼링 없음** — 그 주기 데이터는 유실).
- 응답 본문 형식은 서버 재량(엣지는 상태코드만 사용).

---

## 8. 엣지 실행 명령

```bash
# 전송 없이 JSON만 확인 (스키마 검토용)
./target/release/hdm_paint --ip 192.168.0.10 --post-robot --dry-run

# 실제 전송 (1회)
./target/release/hdm_paint --ip 192.168.0.10 --post-robot

# 2초 주기 전송
./target/release/hdm_paint --ip 192.168.0.10 --post-robot --watch 2

# 레시피와 동시 전송 (각각 별도 URL로 POST)
./target/release/hdm_paint --ip 192.168.0.10 --post-recipe --post-robot --watch 2

# URL 변경
./target/release/hdm_paint --ip 192.168.0.10 --post-robot \
    --robot-url http://192.168.10.30:18080/api/v1/plc/robot
```

## 9. 서버 측 테스트용 curl

```bash
curl -i -X POST http://192.168.10.30:18080/api/v1/plc/robot \
  -H "Content-Type: application/json" \
  -H "x-edge-key: 90K20HlsV3BstN-AVvkUCij0da5M4Kl6" \
  -d @- <<'JSON'
{"edge_id":"edge-line-01","robot_model":"MPX2600","model_no":8,
 "robot":{"command":{"play_mode_select":true,"call_master_job":false,"servo_on":true,"teach_mode_select":false,"auto_ready_ok":true,"external_start":false},
          "state":{"auto_running":true,"ready_ok":true,"auto_stop":false,"auto_run_condition":true,"auto_run_started":true},
          "alarm":{"panel_estop":false,"pendant_estop":false,"fault":false,"battery_fault":false},
          "do":{"work_id_1":true,"external_start":false,"call_master_job":false,"alarm_reset":false,"servo_on":true,"play_mode_select":true,"teach_mode_select":false,"external_hold":false,"job_start":true,"move_clean_pos":false,"move_home_pos_1":false,"move_home_pos_2":false,"cycle_stop":false,"safety_plug":true,"emergency_stop":false,"conveyor_st12":true},
          "di":{"running":true,"servo_on":true,"top_of_master_job":false,"fault":false,"battery_fault":false,"remote_mode_selected":true,"play_mode_selected":true,"teach_mode_selected":false,"home_position":false,"start_permit":true,"spray_ch1":true,"cp_estop":false,"pp_estop":false,"job_in_progress":true,"at_clean_position":false,"level":false}},
 "jig":{"shift_distance":1200,"chattering_guard":30,"start_sig_start_dist":150,"start_sig_end_dist":900,
        "common":{"data_shift_on":false,"work_data_reset":false,"robot_cv_start":true,"robot_cv_start_2":true,"robot_job_start":true,"work_in_not_on":true,"robot_job_start_2":true},
        "stations":[{"no":1,"work_id":8,"work_in":1,"shift_dist":1200,"job_start_dist":150,"send_to_robot":false,"job_start":false,"send_done":true},
                    {"no":2,"work_id":8,"work_in":1,"shift_dist":980,"job_start_dist":150,"send_to_robot":true,"job_start":true,"send_done":false},
                    {"no":3,"work_id":0,"work_in":0,"shift_dist":0,"job_start_dist":150,"send_to_robot":false,"job_start":false,"send_done":false},
                    {"no":4,"work_id":0,"work_in":0,"shift_dist":0,"job_start_dist":150,"send_to_robot":false,"job_start":false,"send_done":false},
                    {"no":5,"work_id":0,"work_in":0,"shift_dist":0,"job_start_dist":150,"send_to_robot":false,"job_start":false,"send_done":false}],
        "completed":{"work_id":7,"work_in":1}},
 "read_errors":[]}
JSON
```

---

## 10. 확인 필요 사항 (현장/서버 공통)

1. **`work_id_1`(`%PX332`) 비트 폭.** 심볼 테이블에는 WORK ID가 이 1비트만 등록돼
   있습니다. 차종 1~8을 표현하려면 물리적으로 `P332~P334`(3비트) 정도가 쓰일
   가능성이 높은데 나머지는 심볼 미등록 상태입니다. **현장 배선/래더 확인 후**
   비트가 더 있으면 `ROBOT_DO_BITS`에 `work_id_2`, `work_id_3`을 추가하고
   서버는 이를 정수로 조합해 저장하도록 확장해야 합니다.
   → 그 전까지는 차종 추적에 `jig.stations[].work_id`(워드값)를 신뢰하세요.
2. **`%PW` 영역 FEnet 연속읽기 허용 여부.** P 영역 읽기가 PLC 설정상 막혀 있으면
   `read_errors`에 `%PW12` 블록만 계속 잡힙니다. 그 경우 `robot.do`/`robot.di`는
   영구 `null`이 되므로, 해당 신호를 미러링하는 M 접점을 래더에 추가해야 합니다.
3. **라우트 경로**가 `POST /api/v1/plc/robot` 로 맞는지 (다르면 `--robot-url`로 교체 가능).
4. **폴링 주기**를 몇 초로 할지. §4.3 edge 검출 정확도와 저장량의 트레이드오프입니다.
   JIG 핸드셰이크 펄스를 놓치지 않으려면 1~2초를 권장합니다.

