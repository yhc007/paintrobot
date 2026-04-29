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

export const api = {
  today: () => getJson<DailyStats>('/api/v1/stats/today'),
  daily: (date: string) => getJson<DailyStats>(`/api/v1/stats/daily?date=${date}`),
  range: (from: string, to: string) =>
    getJson<DailyStats[]>(`/api/v1/stats/range?from=${from}&to=${to}&group_by=day`),
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
