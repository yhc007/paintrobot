import { useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api, DailyStats } from '../lib/api';
import Blueprint from '../components/Blueprint';

function today(): string {
  return new Date().toISOString().slice(0, 10);
}
function daysAgo(n: number): string {
  const d = new Date(); d.setDate(d.getDate() - n);
  return d.toISOString().slice(0, 10);
}
function shift(date: string, n: number): string {
  const d = new Date(`${date}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

const WINDOW_DAYS = 7;

type Preset = '7d' | '30d' | 'month' | 'custom';

export default function Range() {
  const [preset, setPreset] = useState<Preset>('7d');
  const [from, setFrom] = useState(daysAgo(7));
  const [to, setTo] = useState(today());

  const applyPreset = (p: Preset) => {
    setMovedTo(null);
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

  const days = q.data ?? [];
  // 구간 안에 집계된 작업이 하나도 없을 때만 경계를 물어본다 — 이 조회는 전체
  // 스캔이라 비싸므로 평소에는 건드리지 않는다.
  const noData = q.isSuccess && days.every(d => d.total_jobs === 0);
  const bounds = useQuery({
    queryKey: ['stats', 'bounds'],
    queryFn: api.statsBounds,
    enabled: noData,
    staleTime: 5 * 60_000,
  });

  const [movedTo, setMovedTo] = useState<string | null>(null);

  // 빈 구간이면 데이터가 실제로 있는 마지막 날 기준으로 창을 옮긴다.
  useEffect(() => {
    if (!noData) return;
    const last = bounds.data?.last_date;
    const first = bounds.data?.first_date;
    if (!last || !first) return;
    if (to === last || movedTo === last) return;   // 이미 옮겼거나 옮길 필요 없음
    const start = shift(last, -(WINDOW_DAYS - 1));
    setFrom(start < first ? first : start);
    setTo(last);
    setPreset('custom');
    setMovedTo(last);
  }, [noData, bounds.data, to, movedTo]);
  const maxJobs = Math.max(1, ...days.map(d => d.total_jobs));

  const totalByModel = useMemo(() => {
    const acc = new Map<string, { job_count: number; mismatch_count: number }>();
    days.forEach(day => {
      day.models.forEach(m => {
        const cur = acc.get(m.model_no) ?? { job_count: 0, mismatch_count: 0 };
        cur.job_count += m.job_count;
        cur.mismatch_count += m.mismatch_count;
        acc.set(m.model_no, cur);
      });
    });
    return Array.from(acc, ([model_no, v]) => ({ model_no, ...v }))
      .sort((a, b) => b.job_count - a.job_count);
  }, [days]);

  const totalJobs = days.reduce((n, d) => n + d.total_jobs, 0);
  const totalMiss = days.reduce((n, d) => n + d.mismatch_jobs, 0);
  const avgPerDay = days.length > 0 ? totalJobs / days.length : 0;
  const missPct = totalJobs > 0 ? (totalMiss / totalJobs) * 100 : 0;

  const downloadCsv = () => {
    const rows = [['work_date', 'model_no', 'job_count', 'mismatch_count']];
    days.forEach(day => day.models.forEach(m =>
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
    <>
      <div className="page-head">
        <h1 className="page-title">기간 실적</h1>
        <span className="page-note">{from} → {to} · {days.length}일</span>
      </div>

      <Blueprint title="조회 구간">
        <div className="controls">
          <div className="presets">
            <button className={preset === '7d' ? 'on' : ''} onClick={() => applyPreset('7d')}>7일</button>
            <button className={preset === '30d' ? 'on' : ''} onClick={() => applyPreset('30d')}>30일</button>
            <button className={preset === 'month' ? 'on' : ''} onClick={() => applyPreset('month')}>이번달</button>
            <button className={preset === 'custom' ? 'on' : ''} onClick={() => setPreset('custom')}>커스텀</button>
          </div>
          <label>from <input type="date" value={from} onChange={e => { setFrom(e.target.value); setPreset('custom'); setMovedTo(null); }} /></label>
          <label>to <input type="date" value={to} onChange={e => { setTo(e.target.value); setPreset('custom'); setMovedTo(null); }} /></label>
          <button onClick={downloadCsv} disabled={!q.data}>CSV 내려받기</button>
        </div>
      </Blueprint>

      {q.isLoading && <p className="hint">로딩중…</p>}
      {q.error && <p className="err">{String(q.error)}</p>}
      {movedTo && (
        <p className="hint">
          선택한 구간에 집계된 작업이 없어 데이터가 있는 최근 구간({from} → {to})으로 이동했습니다.
          {bounds.data?.first_date && ` 전체 보유 구간 ${bounds.data.first_date} → ${bounds.data.last_date}.`}
        </p>
      )}
      {noData && bounds.isLoading && <p className="hint">데이터가 있는 구간을 찾는 중…</p>}
      {noData && bounds.isSuccess && !bounds.data?.last_date && (
        <p className="hint">아직 집계된 작업이 전혀 없습니다.</p>
      )}

      {q.data && (
        <>
          <Blueprint
            title="일별 생산 실적"
            right={
              <div className="legend">
                <span><i className="ice" />생산</span>
                <span><i className="bad" />불일치</span>
              </div>
            }
          >
            {days.length === 0 && <div className="hint">구간에 집계된 작업이 없습니다.</div>}
            {days.length > 0 && (
              <div className="daybars">
                {days.map(d => (
                  <div className="daybar" key={d.work_date}>
                    <div className="daybar-val">
                      {d.total_jobs}
                      {d.mismatch_jobs > 0 && <span className="sub"> / {d.mismatch_jobs}</span>}
                    </div>
                    <div className="daybar-col">
                      <div
                        className="daybar-fill"
                        style={{ height: `${(d.total_jobs / maxJobs) * 100}%` }}
                      />
                      {d.mismatch_jobs > 0 && (
                        <div
                          className="daybar-fill miss"
                          style={{ height: `${(d.mismatch_jobs / maxJobs) * 100}%` }}
                        />
                      )}
                    </div>
                    <div className="daybar-label">{d.work_date.slice(5)}</div>
                  </div>
                ))}
              </div>
            )}
          </Blueprint>

          <section className="row-2">
            <Blueprint title={`모델별 누적 · 최근 ${days.length}일`}>
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>모델</th>
                      <th style={{ textAlign: 'right' }}>생산</th>
                      <th style={{ textAlign: 'right' }}>불일치</th>
                      <th style={{ textAlign: 'right' }}>불일치율</th>
                      <th style={{ textAlign: 'right' }}>비중</th>
                    </tr>
                  </thead>
                  <tbody>
                    {totalByModel.length === 0 && (
                      <tr><td className="empty-row" colSpan={5}>데이터 없음</td></tr>
                    )}
                    {totalByModel.map(m => (
                      <tr key={m.model_no}>
                        <td><span className="model">{m.model_no}</span></td>
                        <td className="num">{m.job_count}</td>
                        <td className={`num${m.mismatch_count > 0 ? ' bad' : ''}`}>{m.mismatch_count}</td>
                        <td className="num">
                          {m.job_count > 0 ? ((m.mismatch_count / m.job_count) * 100).toFixed(1) : '0.0'}%
                        </td>
                        <td className="num">
                          {totalJobs > 0 ? ((m.job_count / totalJobs) * 100).toFixed(1) : '0.0'}%
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Blueprint>

            <Blueprint title="구간 요약">
              <div className="summary-grid">
                <div className="summary-cell">
                  <div className="gauge-value">{totalJobs}</div>
                  <div className="gauge-label">총 생산 대수</div>
                </div>
                <div className="summary-cell">
                  <div className="gauge-value mid ice">{avgPerDay.toFixed(1)}</div>
                  <div className="gauge-label">일 평균 생산</div>
                </div>
                <div className="summary-cell">
                  <div className={`gauge-value mid${totalMiss > 0 ? ' bad' : ' ok'}`}>{totalMiss}</div>
                  <div className="gauge-label">모델 불일치 건수</div>
                </div>
                <div className="summary-cell">
                  <div className={`gauge-value mid${missPct > 0 ? ' warn' : ' ok'}`}>
                    {missPct.toFixed(1)}<span className="gauge-unit">%</span>
                  </div>
                  <div className="gauge-label">불일치율</div>
                </div>
              </div>
              <div className="bp-foot">
                <span>모델 {totalByModel.length}종</span>
                <span className="push">집계일 {days.length}일</span>
              </div>
            </Blueprint>
          </section>
        </>
      )}
    </>
  );
}
