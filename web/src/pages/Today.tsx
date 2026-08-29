import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, DailyStats, LiveFrame, PlcCurrent } from '../lib/api';
import Blueprint from '../components/Blueprint';
import PlcCard from '../components/PlcCard';
import LivePanel from '../components/LivePanel';
import { buildModelPalette, colorFor } from '../lib/palette';

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

  // PLC↔카메라 지연 상관 추정. 전체 스캔이라 비싸므로 5분에 한 번만.
  // DB는 건드리지 않는 관찰용 값이다.
  const est = useQuery({
    queryKey: ['stats', 'reconcile', stats.data?.work_date],
    queryFn: () => api.reconcile(stats.data!.work_date),
    enabled: !!stats.data?.work_date,
    staleTime: 5 * 60_000,
    refetchInterval: 5 * 60_000,
    retry: false,
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
  // 실적 페이지와 같은 색. 배정이 모델번호에서 나오므로 두 화면이 자동으로 맞는다.
  const palette = buildModelPalette(s.models);
  const maxCount = Math.max(1, ...ranked.map(m => m.job_count));
  const top = ranked[0];
  const missPct = s.total_jobs > 0 ? (s.mismatch_jobs / s.total_jobs) * 100 : 0;
  const clean = s.mismatch_jobs === 0;

  // 엣지가 PLC·카메라를 짝지어 보낸 적이 있는가. 없으면 s.mismatch_jobs는
  // "이상 없음"이 아니라 "비교한 적 없음"이다 — 그 둘을 같은 초록불로
  // 표시하면 검증되지 않은 라인을 정상이라고 말하는 셈이 된다.
  //
  // reconcile이 세는 camera_events는 PLC 타임스탬프가 없는(=짝이 없는) 행이다.
  // 그게 오늘 집계 건수 전부라면 검증된 비교가 하나도 없었다는 뜻.
  const verified = est.data != null && est.data.camera_events < s.total_jobs;
  const estTotal = (est.data?.matched ?? 0) + (est.data?.mismatch ?? 0);
  const estRate = estTotal > 0 ? ((est.data?.matched ?? 0) / estTotal) * 100 : null;

  const verdict = verified
    ? { tone: clean ? 'ok' : 'bad', label: clean ? '정상' : '확인 필요',
        foot: `불일치율 ${missPct.toFixed(1)}%` }
    : est.data?.offset_secs != null
      ? { tone: 'idle', label: '추정',
          foot: `지연 ${(est.data.offset_secs / 60).toFixed(1)}분 기준 · 미확정` }
      : { tone: 'idle', label: '미검증',
          foot: est.isLoading ? '정합 추정 중…' : '표본 부족 · 정합 판정 없음' };

  return (
    <>
      <div className="page-head">
        <h1 className="page-title">금일 현황</h1>
        <span className="page-note">기준일 {s.work_date} · 30초 주기 갱신 · 이벤트 스트림 연결</span>
      </div>

      <section className="today-top">
        <div className="today-kpis">
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
            right={<div className={`verdict ${verdict.tone}`}>{verdict.label}</div>}
            foot={
              <>
                <span>{verdict.foot}</span>
                <span className="push">PLC 지시 ↔ 카메라 인식</span>
              </>
            }
          >
            <div className="match-row">
              <div className={`gauge-value${verified ? (clean ? ' ok' : ' bad') : ''}`}>
                {verified ? s.mismatch_jobs : est.data?.mismatch ?? '—'}
                <span className="gauge-unit"> 건</span>
              </div>
              {!verified && (
                <div className="match-aside">
                  <div>추정 정합 {estRate === null ? '—' : `${estRate.toFixed(1)}%`}</div>
                  <div>표본 {est.data?.matched ?? 0}+{est.data?.mismatch ?? 0}대</div>
                </div>
              )}
            </div>
          </Blueprint>
        </div>

        <div className="today-side">
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
                    className="bar-fill"
                    style={{
                      width: `${(m.job_count / maxCount) * 100}%`,
                      background: colorFor(palette, m.model_no),
                    }}
                  >
                    {/* 불일치는 막대 전체를 빨갛게 칠하지 않고 오른쪽 끝 구간으로만
                        표시한다 — 그래야 모델 색이 살아남는다. */}
                    {m.mismatch_count > 0 && (
                      <span
                        className="bar-miss"
                        style={{ width: `${(m.mismatch_count / m.job_count) * 100}%` }}
                      />
                    )}
                  </span>
                </span>
                <span className="rank-val">{m.job_count}</span>
              </div>
            ))}
          </Blueprint>
        </div>
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
                    <td>
                      <span className="model">
                        <i className="swatch" style={{ background: colorFor(palette, m.model_no) }} />
                        {m.model_no}
                      </span>
                    </td>
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
