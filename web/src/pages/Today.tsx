import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, DailyStats, LiveFrame, PlcCurrent } from '../lib/api';
import Blueprint from '../components/Blueprint';
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

  if (stats.isLoading) return <p className="hint">로딩중…</p>;
  if (stats.error || !stats.data) return <p className="err">데이터를 가져오지 못했습니다.</p>;

  const s = stats.data;
  const cur: PlcCurrent | null | undefined = plc.data;
  const ranked = [...s.models].sort((a, b) => b.job_count - a.job_count);
  const maxCount = Math.max(1, ...ranked.map(m => m.job_count));
  const top = ranked[0];
  const missPct = s.total_jobs > 0 ? (s.mismatch_jobs / s.total_jobs) * 100 : 0;
  const clean = s.mismatch_jobs === 0;

  return (
    <>
      <div className="page-head">
        <h1 className="page-title">금일 현황</h1>
        <span className="page-note">기준일 {s.work_date} · 30초 주기 갱신 · 이벤트 스트림 연결</span>
      </div>

      <section className="row-3">
        <PlcCard plc={cur} />

        <Blueprint
          title="금일 생산 실적"
          right={<div className="verdict idle">누계</div>}
          foot={
            <>
              <span>모델 {s.models.length}종</span>
              {top && <span>최다 {top.model_no} · {top.job_count}대</span>}
              <span className="push">edge → paint.coreon.build</span>
            </>
          }
        >
          <div className="match-row">
            <div className="gauge-value">
              {s.total_jobs}
              <span className="gauge-unit"> 대</span>
            </div>
            <div className="match-aside">
              <div>정상 {s.total_jobs - s.mismatch_jobs}대</div>
              <div>불일치 {s.mismatch_jobs}대</div>
            </div>
          </div>
        </Blueprint>

        <Blueprint
          title="품질 이상 · 모델 불일치"
          right={<div className={`verdict ${clean ? 'ok' : 'bad'}`}>{clean ? '정상' : '확인 필요'}</div>}
          foot={
            <>
              <span>불일치율 {missPct.toFixed(1)}%</span>
              <span className="push">PLC 지시 ↔ 카메라 인식</span>
            </>
          }
        >
          <div className="match-row">
            <div className={`gauge-value${clean ? ' ok' : ' bad'}`}>
              {s.mismatch_jobs}
              <span className="gauge-unit"> 건</span>
            </div>
          </div>
        </Blueprint>
      </section>

      <section className="row-live">
        <Blueprint
          title="투입구 카메라 · 실시간"
          right={<a className="verdict idle" href="/live" style={{ textDecoration: 'none' }}>크게 보기</a>}
        >
          <LivePanel compact />
        </Blueprint>

        <Blueprint title="모델별 생산 순위 · 금일">
          {ranked.length === 0 && <div className="hint">아직 집계된 작업이 없습니다.</div>}
          {ranked.map(m => (
            <div className="rank-row" key={m.model_no}>
              <span className="rank-label">{m.model_no}</span>
              <span className="bar-track">
                <span
                  className={`bar-fill${m.mismatch_count > 0 ? ' bad' : ''}`}
                  style={{ width: `${(m.job_count / maxCount) * 100}%`, display: 'block' }}
                />
              </span>
              <span className="rank-val">{m.job_count}</span>
            </div>
          ))}
        </Blueprint>
      </section>

      <Blueprint title="모델별 상세 · 금일">
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>모델</th>
                <th style={{ textAlign: 'right' }}>생산</th>
                <th style={{ textAlign: 'right' }}>불일치</th>
                <th style={{ textAlign: 'right' }}>불일치율</th>
                <th style={{ textAlign: 'right' }}>비중</th>
                <th>판정</th>
              </tr>
            </thead>
            <tbody>
              {ranked.length === 0 && (
                <tr><td className="empty-row" colSpan={6}>데이터 없음</td></tr>
              )}
              {ranked.map(m => {
                const rate = m.job_count > 0 ? (m.mismatch_count / m.job_count) * 100 : 0;
                const share = s.total_jobs > 0 ? (m.job_count / s.total_jobs) * 100 : 0;
                return (
                  <tr key={m.model_no}>
                    <td><span className="model">{m.model_no}</span></td>
                    <td className="num">{m.job_count}</td>
                    <td className={`num${m.mismatch_count > 0 ? ' bad' : ''}`}>{m.mismatch_count}</td>
                    <td className="num">{rate.toFixed(1)}%</td>
                    <td className="num">{share.toFixed(1)}%</td>
                    <td>
                      <span className={`verdict ${m.mismatch_count > 0 ? 'bad' : 'ok'}`}>
                        {m.mismatch_count > 0 ? '이상' : '정상'}
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Blueprint>
    </>
  );
}
