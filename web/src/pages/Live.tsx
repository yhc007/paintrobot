import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api } from '../lib/api';
import Blueprint from '../components/Blueprint';
import PlcCard, { matchState } from '../components/PlcCard';
import LivePanel from '../components/LivePanel';

const VERDICT_TEXT = {
  match: '정합 OK',
  mismatch: '정합 불일치',
  waiting: '한쪽 신호 대기',
  none: '신호 없음',
} as const;

// 라인·정합 — 투입구 카메라 원본과 PLC 지시값을 나란히 놓고 대조한다.
export default function Live() {
  const plc = useQuery({
    queryKey: ['plc', 'current'],
    queryFn: api.plcCurrent,
    refetchInterval: 10_000,
  });
  const stats = useQuery({
    queryKey: ['stats', 'today'],
    queryFn: api.today,
    refetchInterval: 30_000,
  });

  // 상대 시각("초 전") 갱신용 틱
  const [, force] = useState(0);
  useEffect(() => {
    const id = setInterval(() => force(n => n + 1), 5_000);
    return () => clearInterval(id);
  }, []);

  const state = matchState(plc.data);
  const cls = state === 'match' ? 'ok' : state === 'mismatch' ? 'bad' : 'idle';
  const mismatched = (stats.data?.models ?? [])
    .filter(m => m.mismatch_count > 0)
    .sort((a, b) => b.mismatch_count - a.mismatch_count);

  return (
    <>
      <div className="page-head">
        <h1 className="page-title">라인 · 정합</h1>
        <span className="page-note">현대정밀 R1 도장라인 · 소재 인식 카메라 · 10초 주기 대조</span>
      </div>

      <section className="row-live">
        <Blueprint
          title="투입구 카메라 · 실시간"
          right={<div className="verdict idle">CAM-01 · MSE</div>}
          foot={
            <>
              <span>PLC {plc.data?.model_no ?? '—'}</span>
              <span>인식 {plc.data?.camera_model_no ?? '—'}</span>
              <span className={`push ${cls}`}>{VERDICT_TEXT[state]}</span>
            </>
          }
        >
          <LivePanel />
        </Blueprint>

        <div style={{ display: 'grid', gap: 'var(--pb-gap)', alignContent: 'start' }}>
          <PlcCard plc={plc.data} />

          <Blueprint
            title="모델별 불일치 현황 · 금일"
            right={
              <div className={`verdict ${mismatched.length ? 'bad' : 'ok'}`}>
                {stats.data ? `${stats.data.mismatch_jobs}건` : '—'}
              </div>
            }
          >
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>모델</th>
                    <th style={{ textAlign: 'right' }}>생산</th>
                    <th style={{ textAlign: 'right' }}>불일치</th>
                    <th style={{ textAlign: 'right' }}>비율</th>
                  </tr>
                </thead>
                <tbody>
                  {mismatched.length === 0 && (
                    <tr>
                      <td className="empty-row" colSpan={4}>
                        {stats.data ? '금일 불일치 없음' : '집계 대기'}
                      </td>
                    </tr>
                  )}
                  {mismatched.map(m => (
                    <tr key={m.model_no}>
                      <td><span className="model">{m.model_no}</span></td>
                      <td className="num">{m.job_count}</td>
                      <td className="num bad">{m.mismatch_count}</td>
                      <td className="num">
                        {m.job_count > 0 ? ((m.mismatch_count / m.job_count) * 100).toFixed(1) : '0.0'}%
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="hint" style={{ marginTop: 10 }}>
              건별 정합 이력(시각·행거번호·신뢰도)은 엣지에서 이벤트 단위로 올라오면 이 자리에 붙습니다.
            </p>
          </Blueprint>
        </div>
      </section>

    </>
  );
}
