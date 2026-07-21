import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';
import { api, DailyStats, LiveFrame, PlcCurrent } from '../lib/api';
import KpiCard from '../components/KpiCard';
import PlcCard from '../components/PlcCard';
import LivePanel from '../components/LivePanel';

export default function Today() {
  const qc = useQueryClient();
  const stats = useQuery({
    queryKey: ['stats', 'today'],
    queryFn: api.today,
    refetchInterval: 30_000,
  });
  const plc = useQuery({
    queryKey: ['plc', 'current'],
    queryFn: api.plcCurrent,
    refetchInterval: 30_000,
  });

  // SSE pushes both stats and current_plc together — wired to the same caches.
  const [, force] = useState(0);
  useEffect(() => {
    const es = new EventSource('/api/v1/stream/live');
    es.addEventListener('stats', ev => {
      try {
        const data = JSON.parse((ev as MessageEvent).data) as Partial<LiveFrame> | DailyStats;
        if ('stats' in data && data.stats) {
          qc.setQueryData(['stats', 'today'], data.stats as DailyStats);
          if ('current_plc' in data) {
            qc.setQueryData(['plc', 'current'], (data as LiveFrame).current_plc);
          }
        } else {
          // back-compat: old payload was just DailyStats
          qc.setQueryData(['stats', 'today'], data as DailyStats);
        }
      } catch { /* ignore malformed frames */ }
    });
    // refresh "초 전" relative time every 5s
    const tick = setInterval(() => force(n => n + 1), 5_000);
    return () => { es.close(); clearInterval(tick); };
  }, [qc]);

  if (stats.isLoading) return <p>로딩중…</p>;
  if (stats.error || !stats.data) return <p className="err">데이터를 가져오지 못했습니다.</p>;
  const s = stats.data;
  const cur: PlcCurrent | null | undefined = plc.data;
  const topModels = [...s.models].sort((a, b) => b.job_count - a.job_count).slice(0, 5);
  const maxCount = Math.max(1, ...topModels.map(m => m.job_count));

  return (
    <section>
      <h1>오늘 ({s.work_date})</h1>

      <PlcCard plc={cur} />

      <div className="today-grid">
        <div className="today-main">
          <div className="kpi-row">
            <KpiCard label="총 작업" value={s.total_jobs} />
            <KpiCard label="모델 종류" value={s.models.length} />
            <KpiCard label="불일치" value={s.mismatch_jobs} accent={s.mismatch_jobs > 0 ? 'warn' : 'normal'} />
          </div>

          <h2>모델별 카운트</h2>
          <div className="chart">
            <ResponsiveContainer width="100%" height={320}>
              <BarChart data={s.models}>
                <CartesianGrid strokeDasharray="3 3" stroke="#ddd" />
                <XAxis dataKey="model_no" />
                <YAxis allowDecimals={false} />
                <Tooltip />
                <Bar dataKey="job_count" fill="#4f46e5" name="작업" />
                <Bar dataKey="mismatch_count" fill="#ef4444" name="불일치" />
              </BarChart>
            </ResponsiveContainer>
          </div>

          <h2>모델별 상세</h2>
          <table>
            <thead>
              <tr>
                <th>모델</th>
                <th>작업 수</th>
                <th>불일치</th>
              </tr>
            </thead>
            <tbody>
              {s.models.map(m => (
                <tr key={m.model_no}>
                  <td>{m.model_no}</td>
                  <td>{m.job_count}</td>
                  <td className={m.mismatch_count > 0 ? 'warn' : ''}>{m.mismatch_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <aside className="today-side">
          <div className="side-head">
            <span className="side-title">실시간 영상</span>
            <a className="side-link" href="/live">크게 보기</a>
          </div>
          <LivePanel compact />

          <div className="side-panel">
            <div className="side-panel-head">오늘 요약</div>
            <div className="side-stat"><span>총 작업</span><b>{s.total_jobs}</b></div>
            <div className="side-stat"><span>모델 종류</span><b>{s.models.length}</b></div>
            <div className="side-stat">
              <span className={s.mismatch_jobs > 0 ? 'warn' : ''}>불일치</span>
              <b className={s.mismatch_jobs > 0 ? 'warn' : ''}>{s.mismatch_jobs}</b>
            </div>

            <div className="side-sub">모델별 TOP</div>
            {topModels.length === 0 && <div className="side-empty">아직 작업 없음</div>}
            {topModels.map(m => (
              <div className="side-bar-row" key={m.model_no}>
                <span className="side-bar-label">{m.model_no}</span>
                <span className="side-bar-track">
                  <span
                    className={`side-bar-fill${m.mismatch_count > 0 ? ' warn' : ''}`}
                    style={{ width: `${(m.job_count / maxCount) * 100}%` }}
                  />
                </span>
                <span className="side-bar-val">{m.job_count}</span>
              </div>
            ))}
          </div>
        </aside>
      </div>
    </section>
  );
}
