import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer,
} from 'recharts';
import { api, DailyStats } from '../lib/api';

function today(): string {
  return new Date().toISOString().slice(0, 10);
}
function daysAgo(n: number): string {
  const d = new Date(); d.setDate(d.getDate() - n);
  return d.toISOString().slice(0, 10);
}

type Preset = '7d' | '30d' | 'month' | 'custom';

export default function Range() {
  const [preset, setPreset] = useState<Preset>('7d');
  const [from, setFrom] = useState(daysAgo(7));
  const [to, setTo] = useState(today());

  const applyPreset = (p: Preset) => {
    setPreset(p);
    if (p === '7d') { setFrom(daysAgo(7)); setTo(today()); }
    else if (p === '30d') { setFrom(daysAgo(30)); setTo(today()); }
    else if (p === 'month') {
      const d = new Date(); d.setDate(1);
      setFrom(d.toISOString().slice(0, 10)); setTo(today());
    }
  };

  const q = useQuery<DailyStats[]>({
    queryKey: ['stats', 'range', from, to],
    queryFn: () => api.range(from, to),
  });

  const totalByModel = useMemo(() => {
    const acc = new Map<string, { job_count: number; mismatch_count: number }>();
    (q.data ?? []).forEach(day => {
      day.models.forEach(m => {
        const cur = acc.get(m.model_no) ?? { job_count: 0, mismatch_count: 0 };
        cur.job_count += m.job_count;
        cur.mismatch_count += m.mismatch_count;
        acc.set(m.model_no, cur);
      });
    });
    return Array.from(acc, ([model_no, v]) => ({ model_no, ...v }))
      .sort((a, b) => b.job_count - a.job_count);
  }, [q.data]);

  const downloadCsv = () => {
    const rows = [['work_date', 'model_no', 'job_count', 'mismatch_count']];
    (q.data ?? []).forEach(day => day.models.forEach(m =>
      rows.push([day.work_date, m.model_no, String(m.job_count), String(m.mismatch_count)])
    ));
    const csv = rows.map(r => r.join(',')).join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = `paintrobot_${from}_${to}.csv`; a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <section>
      <h1>기간 통계</h1>
      <div className="range-controls">
        <div className="presets">
          <button className={preset === '7d' ? 'on' : ''} onClick={() => applyPreset('7d')}>7일</button>
          <button className={preset === '30d' ? 'on' : ''} onClick={() => applyPreset('30d')}>30일</button>
          <button className={preset === 'month' ? 'on' : ''} onClick={() => applyPreset('month')}>이번달</button>
          <button className={preset === 'custom' ? 'on' : ''} onClick={() => setPreset('custom')}>커스텀</button>
        </div>
        <label>from <input type="date" value={from} onChange={e => { setFrom(e.target.value); setPreset('custom'); }} /></label>
        <label>to <input type="date" value={to} onChange={e => { setTo(e.target.value); setPreset('custom'); }} /></label>
        <button onClick={downloadCsv} disabled={!q.data}>CSV</button>
      </div>

      {q.isLoading && <p>로딩중…</p>}
      {q.error && <p className="err">{String(q.error)}</p>}

      {q.data && (
        <>
          <h2>일별 추이</h2>
          <div className="chart">
            <ResponsiveContainer width="100%" height={320}>
              <LineChart data={q.data}>
                <CartesianGrid strokeDasharray="3 3" stroke="#ddd" />
                <XAxis dataKey="work_date" />
                <YAxis allowDecimals={false} />
                <Tooltip /><Legend />
                <Line type="monotone" dataKey="total_jobs" stroke="#4f46e5" name="작업" />
                <Line type="monotone" dataKey="mismatch_jobs" stroke="#ef4444" name="불일치" />
              </LineChart>
            </ResponsiveContainer>
          </div>

          <h2>모델별 합계</h2>
          <table>
            <thead><tr><th>모델</th><th>작업 수</th><th>불일치</th></tr></thead>
            <tbody>
              {totalByModel.map(m => (
                <tr key={m.model_no}>
                  <td>{m.model_no}</td>
                  <td>{m.job_count}</td>
                  <td className={m.mismatch_count > 0 ? 'warn' : ''}>{m.mismatch_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
