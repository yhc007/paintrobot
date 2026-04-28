import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';
import { api, DailyStats, LiveFrame, PlcCurrent } from '../lib/api';
import KpiCard from '../components/KpiCard';
import PlcCard from '../components/PlcCard';

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

  return (
    <section>
      <h1>오늘 ({s.work_date})</h1>

      <PlcCard plc={cur} />

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
    </section>
  );
}
