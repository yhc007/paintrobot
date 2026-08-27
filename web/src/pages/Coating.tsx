import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer,
  ReferenceLine,
} from 'recharts';
import { api, postCoating, CoatingSample } from '../lib/api';
import Blueprint from '../components/Blueprint';
import KpiCard from '../components/KpiCard';

const ICE = '#94bce3';
const OK = '#6fcf97';
const WARN = '#e8a33d';
const BAD = '#d9614c';
const STEEL = '#5980a6';
const GRID = 'rgba(230,237,244,0.12)';

// 판정 기준은 API가 주지 않아 UI에서 정한다 — 목표 두께 대비 ±10%.
const TOL_PCT = 10;

function fmtTime(ms: number) {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function sample(s: CoatingSample) {
  return {
    t: fmtTime(s.measured_at),
    measured_um: s.measured_um,
    target_um: s.target_um,
    current: s.current_pressure,
    recommended: s.recommended_pressure,
    temp: s.temperature_c,
    hum: s.humidity_pct,
    model_no: s.model_no,
  };
}

type Spec = {
  model: string;
  n: number;
  target: number;
  meas: number;
  dev: number;
  bar: number;
};

export default function Coating() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ['coatings', 'recent'],
    queryFn: () => api.coatingsRecent(200),
    refetchInterval: 5_000,
  });

  const data = useMemo(() => (q.data?.series ?? []).map(sample), [q.data]);
  const latest = data[data.length - 1];

  // 모델별 도막 규격 — 최근 샘플을 모델 단위로 접는다.
  const specs: Spec[] = useMemo(() => {
    const acc = new Map<string, { n: number; target: number; meas: number; bar: number }>();
    (q.data?.series ?? []).forEach(s => {
      const cur = acc.get(s.model_no) ?? { n: 0, target: 0, meas: 0, bar: 0 };
      cur.n += 1;
      cur.target += s.target_um;
      cur.meas += s.measured_um;
      cur.bar += s.recommended_pressure;
      acc.set(s.model_no, cur);
    });
    return Array.from(acc, ([model, v]) => {
      const target = v.target / v.n;
      const meas = v.meas / v.n;
      return { model, n: v.n, target, meas, dev: meas - target, bar: v.bar / v.n };
    }).sort((a, b) => b.n - a.n);
  }, [q.data]);

  const devOk = (s: Spec) => s.target > 0 && Math.abs(s.dev) <= (s.target * TOL_PCT) / 100;

  const empty = data.length === 0;

  // 최근 측정의 압력 보정 폭
  const delta = latest ? latest.recommended - latest.current : 0;

  // Tester form (manual sender for verifying the loop)
  const [model, setModel] = useState('HD-A120');
  const [measured, setMeasured] = useState(28);
  const [target, setTarget] = useState(30);
  const [pressure, setPressure] = useState(3.5);
  const [edgeKey, setEdgeKey] = useState(() => localStorage.getItem('edgeKey') ?? '');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true); setErr(null);
    try {
      localStorage.setItem('edgeKey', edgeKey);
      await postCoating({
        event_id: `web-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        model_no: model,
        measured_um: measured,
        target_um: target,
        current_pressure: pressure,
      }, edgeKey);
      qc.invalidateQueries({ queryKey: ['coatings', 'recent'] });
    } catch (e: any) {
      setErr(String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="page-head">
        <h1 className="page-title">도막 · 레시피</h1>
        <span className="page-note">
          도막 두께 ↔ 분사 압력 보정 · 5초 주기 갱신 · 판정 기준 목표 대비 ±{TOL_PCT}%
        </span>
      </div>

      <section className="row-4">
        <KpiCard
          label="현재 모델"
          value={latest ? latest.model_no : '—'}
          accent={latest ? 'normal' : 'idle'}
          sub={latest ? `최근 측정 ${latest.t}` : '측정 대기'}
        />
        <KpiCard
          label="목표 두께"
          value={latest ? latest.target_um.toFixed(0) : '—'}
          unit={latest ? 'μm' : undefined}
          accent={latest ? 'normal' : 'idle'}
          sub={latest ? `허용 ±${((latest.target_um * TOL_PCT) / 100).toFixed(1)} μm` : '측정 대기'}
        />
        <KpiCard
          label="최근 측정"
          value={latest ? latest.measured_um.toFixed(0) : '—'}
          unit={latest ? 'μm' : undefined}
          accent={
            !latest ? 'idle'
              : Math.abs(latest.measured_um - latest.target_um) <= (latest.target_um * TOL_PCT) / 100
                ? 'normal' : 'warn'
          }
          sub={latest
            ? `${latest.measured_um - latest.target_um >= 0 ? '+' : '−'}${Math.abs(latest.measured_um - latest.target_um).toFixed(1)} μm · 목표 대비`
            : '측정 대기'}
        />
        <KpiCard
          label="권장 분사 압력"
          value={latest ? latest.recommended.toFixed(2) : '—'}
          unit={latest ? 'bar' : undefined}
          accent={!latest ? 'idle' : Math.abs(delta) > 0.2 ? 'warn' : 'ice'}
          sub={latest
            ? `현재 ${latest.current.toFixed(2)} bar → ${delta >= 0 ? '+' : ''}${delta.toFixed(2)} 조정`
            : '측정 대기'}
        />
      </section>

      <section className="row-2">
        <Blueprint title="도막 두께 추이 · μm">
          {empty ? <div className="chart-empty">측정 데이터 없음</div> : <div className="chart">
            <ResponsiveContainer width="100%" height={260}>
              <LineChart data={data} margin={{ top: 12, right: 8, bottom: 0, left: -12 }}>
                <CartesianGrid stroke={GRID} />
                <XAxis dataKey="t" stroke={STEEL} tickLine={false} minTickGap={40} />
                <YAxis allowDecimals={false} unit="μm" stroke={STEEL} tickLine={false} />
                <Tooltip cursor={{ stroke: STEEL }} />
                <Legend />
                <Line type="monotone" dataKey="measured_um" stroke={ICE} name="측정" dot={false} strokeWidth={2} />
                <Line type="monotone" dataKey="target_um" stroke={OK} name="목표" dot={false} strokeDasharray="4 4" />
              </LineChart>
            </ResponsiveContainer>
          </div>}
        </Blueprint>

        <Blueprint title="분사 압력 · bar">
          {empty ? <div className="chart-empty">측정 데이터 없음</div> : <div className="chart">
            <ResponsiveContainer width="100%" height={260}>
              <LineChart data={data} margin={{ top: 12, right: 8, bottom: 0, left: -12 }}>
                <CartesianGrid stroke={GRID} />
                <XAxis dataKey="t" stroke={STEEL} tickLine={false} minTickGap={40} />
                <YAxis domain={[1, 6]} unit=" bar" stroke={STEEL} tickLine={false} />
                <Tooltip cursor={{ stroke: STEEL }} />
                <Legend />
                <ReferenceLine y={1} stroke={GRID} strokeDasharray="2 2" />
                <ReferenceLine y={6} stroke={GRID} strokeDasharray="2 2" />
                <Line type="monotone" dataKey="current" stroke={STEEL} name="현재" dot={false} />
                <Line type="monotone" dataKey="recommended" stroke={BAD} name="권장" dot={false} strokeWidth={2} />
              </LineChart>
            </ResponsiveContainer>
          </div>}
        </Blueprint>
      </section>

      <Blueprint title="부스 온도 / 습도 · 분사 시점">
        {empty ? <div className="chart-empty">측정 데이터 없음</div> : <div className="chart">
          <ResponsiveContainer width="100%" height={200}>
            <LineChart data={data} margin={{ top: 12, right: 0, bottom: 0, left: -12 }}>
              <CartesianGrid stroke={GRID} />
              <XAxis dataKey="t" stroke={STEEL} tickLine={false} minTickGap={40} />
              <YAxis yAxisId="t" orientation="left" domain={['auto', 'auto']} unit="°C" stroke={STEEL} tickLine={false} />
              <YAxis yAxisId="h" orientation="right" domain={[0, 100]} unit="%" stroke={STEEL} tickLine={false} />
              <Tooltip cursor={{ stroke: STEEL }} />
              <Legend />
              <Line yAxisId="t" type="monotone" dataKey="temp" stroke={WARN} name="온도(°C)" dot={false} />
              <Line yAxisId="h" type="monotone" dataKey="hum" stroke={ICE} name="습도(%)" dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>}
      </Blueprint>

      <Blueprint
        title="모델별 도막 규격 · 최근 측정"
        right={<div className="verdict idle">{specs.length}개 모델 · {data.length}건</div>}
      >
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>모델</th>
                <th style={{ textAlign: 'right' }}>측정 건수</th>
                <th style={{ textAlign: 'right' }}>목표 (μm)</th>
                <th style={{ textAlign: 'right' }}>평균 측정</th>
                <th style={{ textAlign: 'right' }}>편차</th>
                <th style={{ textAlign: 'right' }}>권장 압력</th>
                <th>판정</th>
              </tr>
            </thead>
            <tbody>
              {specs.length === 0 && (
                <tr><td className="empty-row" colSpan={7}>데이터 없음</td></tr>
              )}
              {specs.map(p => {
                const good = devOk(p);
                return (
                  <tr key={p.model}>
                    <td><span className="model">{p.model}</span></td>
                    <td className="num">{p.n}</td>
                    <td className="num">{p.target.toFixed(1)}</td>
                    <td className="num">{p.meas.toFixed(1)}</td>
                    <td className={`num${good ? '' : ' warn'}`}>
                      {p.dev >= 0 ? '+' : '−'}{Math.abs(p.dev).toFixed(1)}
                    </td>
                    <td className="num">{p.bar.toFixed(2)} bar</td>
                    <td><span className={`verdict ${good ? 'ok' : 'warn'}`}>{good ? '정상' : '주의'}</span></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Blueprint>

      <Blueprint title="최근 샘플 · 20건">
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>시각</th><th>모델</th>
                <th style={{ textAlign: 'right' }}>측정(μm)</th>
                <th style={{ textAlign: 'right' }}>목표(μm)</th>
                <th style={{ textAlign: 'right' }}>현재(bar)</th>
                <th style={{ textAlign: 'right' }}>권장(bar)</th>
                <th style={{ textAlign: 'right' }}>T(°C)</th>
                <th style={{ textAlign: 'right' }}>H(%)</th>
              </tr>
            </thead>
            <tbody>
              {data.length === 0 && (
                <tr><td className="empty-row" colSpan={8}>데이터 없음</td></tr>
              )}
              {data.slice().reverse().slice(0, 20).map((r, i) => {
                const d = r.recommended - r.current;
                return (
                  <tr key={i}>
                    <td>{r.t}</td>
                    <td><span className="model">{r.model_no}</span></td>
                    <td className="num">{r.measured_um.toFixed(1)}</td>
                    <td className="num">{r.target_um.toFixed(1)}</td>
                    <td className="num">{r.current.toFixed(2)}</td>
                    <td className={`num${Math.abs(d) > 0.2 ? ' warn' : ''}`}>
                      {r.recommended.toFixed(2)} ({d >= 0 ? '+' : ''}{d.toFixed(2)})
                    </td>
                    <td className="num">{r.temp.toFixed(1)}</td>
                    <td className="num">{r.hum.toFixed(0)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Blueprint>

      <Blueprint title="테스트 송신">
        <p className="hint" style={{ marginBottom: 12 }}>
          아래 폼은 엣지 PC가 보낼 측정값을 직접 흉내 냅니다. 엣지 키는 브라우저 localStorage에만 저장됩니다.
        </p>
        <div className="sender">
          <label>모델 <input value={model} onChange={e => setModel(e.target.value)} /></label>
          <label>측정 두께(μm) <input type="number" value={measured} onChange={e => setMeasured(Number(e.target.value))} /></label>
          <label>목표 두께(μm) <input type="number" value={target} onChange={e => setTarget(Number(e.target.value))} /></label>
          <label>현재 압력(bar) <input type="number" step="0.1" value={pressure} onChange={e => setPressure(Number(e.target.value))} /></label>
          <label>X-Edge-Key <input type="password" value={edgeKey} onChange={e => setEdgeKey(e.target.value)} /></label>
          <button onClick={submit} disabled={busy || !edgeKey}>{busy ? '전송중…' : '전송'}</button>
        </div>
        {err && <p className="err" style={{ marginTop: 10 }}>{err}</p>}
      </Blueprint>
    </>
  );
}
