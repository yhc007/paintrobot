import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';
import { api, DailyStats } from '../lib/api';
import KpiCard from '../components/KpiCard';

export default function Today() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ['stats', 'today'],
    queryFn: api.today,
    // SSE feeds near-realtime updates. Polling is a safety net for reconnects.
    refetchInterval: 30_000,
  });

  useEffect(() => {
    const es = new EventSource('/api/v1/stream/live');
    es.addEventListener('stats', ev => {
      try {
        const data = JSON.parse((ev as MessageEvent).data) as DailyStats;
        qc.setQueryData(['stats', 'today'], data);
      } catch {
        // ignore malformed frames
      }
    });
    return () => es.close();
  }, [qc]);

  if (q.isLoading) return <p>로딩중…</p>;
  if (q.error || !q.data) return <p className="err">데이터를 가져오지 못했습니다.</p>;
  const s = q.data;

  return (
    <section>
      <h1>오늘 ({s.work_date})</h1>
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
