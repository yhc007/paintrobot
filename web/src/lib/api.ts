// All fetches go through same-origin paths so CORS is never a problem
// (both dev proxy and prod hit paint.coreon.build).

export type ModelCount = {
  model_no: string;
  job_count: number;
  mismatch_count: number;
};

export type DailyStats = {
  work_date: string;
  total_jobs: number;
  mismatch_jobs: number;
  models: ModelCount[];
};

/// 집계된 작업이 실제로 존재하는 첫/마지막 날짜. 둘 다 null이면 데이터가 하나도 없다.
export type StatsBounds = {
  first_date: string | null;
  last_date: string | null;
};

export type WeatherCurrent = {
  location_name: string;
  lat: number;
  lon: number;
  observed_at: string;
  temperature_c: number;
  humidity_pct: number;
  source: string;
};

async function getJson<T>(path: string): Promise<T> {
  const r = await fetch(path, { headers: { accept: 'application/json' } });
  if (!r.ok) throw new Error(`${path}: ${r.status} ${r.statusText}`);
  return r.json() as Promise<T>;
}

export type PlcCurrent = {
  model_no: string | null;
  edge_id: string | null;
  plc_ts: number | null;
  event_id: string | null;
  camera_model_no: string | null;
  camera_ts: number | null;
};

export type LiveFrame = {
  stats: DailyStats;
  current_plc: PlcCurrent;
};

/// PLC↔카메라 지연 상관의 사후 추정치. DB의 match_status는 건드리지 않는다 —
/// 어디까지나 관찰용이고, `offset_secs`가 null이면 추정 자체를 못 한 것이다.
export type MatchBucket = {
  matched: number;
  mismatch: number;
  total: number;
  /// 표본이 없으면 null. 0으로 내려오면 "이상 없음"으로 오해된다.
  mismatch_rate: number | null;
};

export type ReconcileEstimate = {
  work_date: string;
  offset_secs: number | null;
  /// 전환 직후 구간별. 구간은 카메라 쪽 런 위치라 추정 오프셋과 무관하다.
  after_changeover: {
    first_unit: MatchBucket;
    early_units: MatchBucket;
    steady_units: MatchBucket;
  };
  plc_states: number;
  camera_events: number;
  matched: number;
  mismatch: number;
  skipped_low_confidence: number;
  skipped_no_plc: number;
};

/// 순서에서만 나오는 값들. 일자별 합계로는 혼류가 보이지 않는다.
export type ProductionRun = { model_no: string; count: number; start_ms: number };
export type MixFlow = {
  work_date: string;
  units: number;
  models: number;
  /// 앞 차와 모델이 달라진 횟수
  changeovers: number;
  /// 전환 / (대수-1). 1에 가까울수록 매 대마다 차종이 바뀐다.
  changeover_rate: number;
  avg_run: number;
  max_run: number;
  /// 1대만 끼어든 투입
  singles: number;
  runs: ProductionRun[];
};

/// PLC가 보내온 차종 도장 레시피. 단계(level)마다 저장값(table)과
/// 실제 적용값(applied)이 따로 온다.
export type RecipeAxis = { table: number[]; applied: number[] };
export type PlcRecipe = {
  edge_id?: string | null;
  model_no: number | null;
  model_name?: string | null;
  levels?: number | null;
  received_at?: number | null;
  work_date?: string | null;
  recipe: {
    atomization: RecipeAxis;
    pattern: RecipeAxis;
    flow: RecipeAxis;
  } | null;
};

/// R1 로봇(MPX2600) 인터페이스. 스펙은 저장소 루트의 `plc_r.md`.
///
/// **비트는 true/false/null 3-state다.** null은 "해당 워드 블록 읽기 실패 =
/// 값 모름"이지 false가 아니다. 비상정지 비트가 null인데 false로 접히면
/// 화면이 "안전함"으로 읽힌다. 그래서 `boolean | null`을 끝까지 유지한다.
export type Bit = boolean | null;
export type BitMap = Record<string, Bit>;

export type JigStation = {
  no: number;
  /// 이 JIG에 실린 차체의 차종번호. 컨베이어를 따라 차체와 같이 움직인다.
  work_id: number | null;
  /// 0 = 워크 없음
  work_in: number | null;
  shift_dist: number | null;
  job_start_dist: number | null;
  send_to_robot: Bit;
  job_start: Bit;
  send_done: Bit;
};

export type RobotCurrent = {
  edge_id: string | null;
  robot_model?: string | null;
  /// HMI 차종번호 %DW5000
  model_no?: number | null;
  received_at: number | null;
  degraded?: boolean;
  read_errors?: string[];
  robot?: {
    command: BitMap;
    state: BitMap;
    alarm: BitMap;
    do: BitMap;
    di: BitMap;
  };
  jig?: {
    shift_distance: number | null;
    chattering_guard: number | null;
    start_sig_start_dist: number | null;
    start_sig_end_dist: number | null;
    common: BitMap;
    stations: JigStation[];
    completed: { work_id: number | null; work_in: number | null };
  };
  /// 서버가 미리 판정해 둔 값. 화면이 같은 계산을 다시 하지 않도록.
  derived?: {
    active_jig: number | null;
    active_work_id: number | null;
    /// true=오도장 위험, false=일치, **null=판정 불가**(정상이 아니다).
    work_id_mismatch: boolean | null;
    /// 내부 릴레이와 출력 접점이 어긋난 키
    io_disagree: string[];
    io_streak: number;
    faults: string[];
    model_no_out_of_range: boolean;
    /// %DW 블록이 설정값까지 전부 0. 라인이 비어서가 아니라 주소가 어긋난
    /// 것일 수 있으므로, 화면은 JIG를 "비어 있음"으로 단정하면 안 된다.
    jig_words_all_zero: boolean;
  };
};

export type RobotEvent = {
  kind: string;
  ts_ms: number;
  jig_no: number | null;
  model_no: number | null;
  detail: string;
  alert: boolean;
};

export type RobotEventDay = {
  work_date: string;
  events: RobotEvent[];
  /// 종류별 누계. `events`는 최근 것만 남기므로 건수는 여기서 본다.
  counts: Record<string, number>;
};

export const api = {
  today: () => getJson<DailyStats>('/api/v1/stats/today'),
  daily: (date: string) => getJson<DailyStats>(`/api/v1/stats/daily?date=${date}`),
  range: (from: string, to: string) =>
    getJson<DailyStats[]>(`/api/v1/stats/range?from=${from}&to=${to}&group_by=day`),
  statsBounds: () => getJson<StatsBounds>('/api/v1/stats/bounds'),
  mixflow: (date: string) => getJson<MixFlow>(`/api/v1/stats/mixflow?date=${date}`),
  reconcile: (date: string) =>
    getJson<ReconcileEstimate>(`/api/v1/stats/reconcile?date=${date}`),
  weather: () => getJson<WeatherCurrent>('/api/v1/weather/current'),
  plcCurrent: () => getJson<PlcCurrent>('/api/v1/plc/current'),
  recipeCurrent: () => getJson<PlcRecipe>('/api/v1/plc/recipe/current'),
  recipeList: () => getJson<PlcRecipe[]>('/api/v1/plc/recipe/list'),
  robotCurrent: () => getJson<RobotCurrent>('/api/v1/plc/robot/current'),
  robotEvents: (date?: string) =>
    getJson<RobotEventDay>(`/api/v1/plc/robot/events${date ? `?date=${date}` : ''}`),
};

/// 레시피를 서버에 저장한다. 쓰기라 엣지 키가 필요하다.
///
/// 수신 엔드포인트를 그대로 쓴다 — 같은 (엣지, 모델)이면 덮어쓰므로, 나중에
/// PLC가 실제 값을 보내오면 그쪽이 이긴다.
export async function saveRecipe(
  payload: {
    edge_id: string;
    model_no: number;
    model_name: string;
    levels: number;
    recipe: NonNullable<PlcRecipe['recipe']>;
  },
  edgeKey: string,
) {
  const r = await fetch('/api/v1/plc/recipe', {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-edge-key': edgeKey },
    body: JSON.stringify(payload),
  });
  if (!r.ok) throw new Error(`저장 실패: ${r.status} ${await r.text()}`);
  return r.json() as Promise<unknown>;
}
