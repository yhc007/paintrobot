// PLC 심볼 이름 → 화면 라벨.
//
// 엣지가 보내는 키는 `plc_r.md` §3의 심볼 테이블에서 온 영문 약칭이다.
// 현장에서 읽는 것은 한글이라 여기서 한 번 옮긴다. 표에 없는 키가 와도
// 키 그대로 보여준다 — 스펙 §2가 forward-compatible을 요구하므로, 나중에
// 심볼이 추가돼도 화면이 조용히 빠뜨리면 안 된다.

export const LABELS: Record<string, string> = {
  // robot.command — PLC → 로봇 지령 (내부 릴레이 M)
  play_mode_select: 'PLAY 모드',
  call_master_job: '마스터 JOB 호출',
  servo_on: '서보 ON',
  teach_mode_select: 'TEACH 모드',
  auto_ready_ok: 'AUTO READY OK',
  external_start: '외부 기동',

  // robot.state — 로봇 운전 상태
  auto_running: '자동운전중',
  ready_ok: 'READY OK',
  auto_stop: '자동정지',
  auto_run_condition: '자동운전 조건만족',
  auto_run_started: '자동운전 시작',

  // robot.alarm
  panel_estop: 'PANEL 비상정지',
  pendant_estop: 'PENDANT 비상정지',
  fault: '이상',
  battery_fault: 'BATTERY 이상',

  // robot.do — PLC → 로봇 출력 접점 (P 영역)
  work_id_1: 'WORK ID.1',
  alarm_reset: '알람 리셋',
  external_hold: '외부 HOLD',
  job_start: 'JOB START',
  move_clean_pos: '세정위치 이동',
  move_home_pos_1: '홈위치 이동 1',
  move_home_pos_2: '홈위치 이동 2',
  cycle_stop: 'CYCLE STOP',
  safety_plug: '세이프티 플러그',
  emergency_stop: '비상정지',
  conveyor_st12: 'C/V ST12',

  // robot.di — 로봇 → PLC 입력 접점
  running: '운전중',
  top_of_master_job: '마스터 JOB 선두',
  remote_mode_selected: 'REMOTE 모드',
  play_mode_selected: 'PLAY 모드 선택됨',
  teach_mode_selected: 'TEACH 모드 선택됨',
  home_position: '홈 위치',
  start_permit: '기동 허가',
  spray_ch1: '분사 CH1',
  cp_estop: 'C.P 비상정지',
  pp_estop: 'P.P 비상정지',
  job_in_progress: 'JOB 수행중',
  at_clean_position: '세정위치 도달',
  level: '레벨',

  // jig.common — 시프트 공통
  data_shift_on: 'DATA SHIFT ON',
  work_data_reset: 'WORK DATA RESET',
  robot_cv_start: 'ROBOT CV START',
  robot_cv_start_2: 'ROBOT CV START 2',
  robot_job_start: 'ROBOT JOB START',
  work_in_not_on: 'WORK IN/NOT ON',
  robot_job_start_2: 'ROBOT JOB START 2',
};

export function label(key: string): string {
  return LABELS[key] ?? key;
}

/// 비상정지·이상 계열 이름인가.
export function isFaultName(key: string): boolean {
  return key.includes('estop') || key.includes('fault') || key === 'emergency_stop';
}

/// 비트 하나를 어떤 색으로 그릴지. **극성을 아는 것만 빨강으로 칠한다.**
///
/// `alarm` 그룹은 PLC 내부 릴레이라 래더가 세우는 값이고, 스펙 §3.3이
/// 이름부터 "로봇 알람"이라 ON = 이상이 맞는다.
///
/// `do`/`di`의 P 영역 접점은 다르다. 현장 실측에서 로봇이 홈 위치·서보 OFF·
/// TEACH 모드로 정상 대기 중인데 `di.cp_estop`과 `di.pp_estop`이 둘 다 ON이고,
/// 같은 순간 PLC 알람 릴레이는 전부 OFF였다. 비상정지 접점은 보통 NC(평상시
/// 닫힘, fail-safe)라 회로가 멀쩡하면 ON으로 읽힌다 — 그 모양과 맞는다.
/// 극성이 현장에서 확인될 때까지 빨강으로 칠하지 않고 "극성 미확인"으로 둔다.
export function bitTone(group: string, key: string): 'fault' | 'unconfirmed' | 'plain' {
  if (!isFaultName(key)) return 'plain';
  return group === 'alarm' ? 'fault' : 'unconfirmed';
}

/// 파생 이벤트 종류 → 한 줄 이름.
export const EVENT_LABELS: Record<string, string> = {
  jig_handoff: 'JIG 전송 시작',
  jig_handoff_done: 'JIG 전송 완료',
  robot_job_started: '로봇 JOB 착수',
  robot_job_ended: '로봇 JOB 종료',
  cycle_completed: '1사이클 완료',
  robot_fault: '로봇 알람',
  estop: '비상정지',
  work_id_mismatch: '오도장 위험',
  io_disagree: '지령↔출력 불일치',
};
