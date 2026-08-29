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

export type CoatingSample = {
  event_id: string;
  model_no: string;
  measured_um: number;
  target_um: number;
  current_pressure: number;
  recommended_pressure: number;
  temperature_c: number;
  humidity_pct: number;
  measured_at: number;
};

export type CoatingsToday = {
  work_date: string;
  total: number;
  avg_measured_um: number;
  avg_recommended_pressure: number;
  series: CoatingSample[];
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
  coatingsToday: () => getJson<CoatingsToday>('/api/v1/coatings/today'),
  coatingsRecent: (limit = 100) =>
    getJson<CoatingsToday>(`/api/v1/coatings/recent?limit=${limit}`),
};

export async function postCoating(payload: {
  event_id: string;
  model_no: string;
  measured_um: number;
  target_um?: number;
  current_pressure: number;
  temperature_c?: number;
  humidity_pct?: number;
  edge_id?: string;
}, edgeKey: string) {
  const r = await fetch('/api/v1/coatings', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-edge-key': edgeKey,
    },
    body: JSON.stringify(payload),
  });
  if (!r.ok) throw new Error(`coatings: ${r.status} ${await r.text()}`);
  return r.json() as Promise<unknown>;
}
