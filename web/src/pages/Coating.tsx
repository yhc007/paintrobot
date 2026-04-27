import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer,
  ReferenceLine,
} from 'recharts';
import { api, postCoating, CoatingSample } from '../lib/api';
import KpiCard from '../components/KpiCard';

function fmtTime(ms: number) {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`;
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

export default function Coating() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ['coatings', 'recent'],
    queryFn: () => api.coatingsRecent(200),
    refetchInterval: 5_000,
  });

  const data = useMemo(() => (q.data?.series ?? []).map(sample), [q.data]);
  const latest = data[data.length - 1];

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
    <section>
      <h1>도막 두께 ↔ 압력 계산</h1>

      <div className="kpi-row">
        <KpiCard
          label="최근 측정 두께"
          value={latest ? `${latest.measured_um.toFixed(1)}μm` : '-'}
        />
        <KpiCard
          label="목표 두께"
          value={latest ? `${latest.target_um.toFixed(1)}μm` : '-'}
        />
        <KpiCard
          label="권장 분사 압력"
          value={latest ? `${latest.recommended.toFixed(2)} bar` : '-'}
          accent={latest && Math.abs(latest.recommended - latest.current) > 0.2 ? 'warn' : 'normal'}
        />
      </div>

      <h2>두께 추이 (μm)</h2>
      <div className="chart">
        <ResponsiveContainer width="100%" height={260}>
          <LineChart data={data}>
            <CartesianGrid strokeDasharray="3 3" stroke="#ddd" />
            <XAxis dataKey="t" />
            <YAxis allowDecimals={false} unit="μm" />
            <Tooltip /><Legend />
            <Line type="monotone" dataKey="measured_um" stroke="#4f46e5" name="측정" dot={false} />
            <Line type="monotone" dataKey="target_um" stroke="#10b981" name="목표" dot={false} strokeDasharray="4 4" />
          </LineChart>
        </ResponsiveContainer>
      </div>

      <h2>분사 압력 (bar)</h2>
      <div className="chart">
        <ResponsiveContainer width="100%" height={260}>
          <LineChart data={data}>
            <CartesianGrid strokeDasharray="3 3" stroke="#ddd" />
            <XAxis dataKey="t" />
            <YAxis domain={[1, 6]} unit=" bar" />
            <Tooltip /><Legend />
            <ReferenceLine y={1} stroke="#aaa" strokeDasharray="2 2" />
            <ReferenceLine y={6} stroke="#aaa" strokeDasharray="2 2" />
            <Line type="monotone" dataKey="current" stroke="#94a3b8" name="현재" dot={false} />
            <Line type="monotone" dataKey="recommended" stroke="#ef4444" name="권장" dot={false} strokeWidth={2} />
          </LineChart>
        </ResponsiveContainer>
      </div>

      <h2>온도 / 습도 (분사 시점)</h2>
      <div className="chart">
        <ResponsiveContainer width="100%" height={220}>
          <LineChart data={data}>
            <CartesianGrid strokeDasharray="3 3" stroke="#ddd" />
            <XAxis dataKey="t" />
            <YAxis yAxisId="t" orientation="left" domain={['auto', 'auto']} unit="°C" />
            <YAxis yAxisId="h" orientation="right" domain={[0, 100]} unit="%" />
            <Tooltip /><Legend />
            <Line yAxisId="t" type="monotone" dataKey="temp" stroke="#f59e0b" name="온도(°C)" dot={false} />
            <Line yAxisId="h" type="monotone" dataKey="hum"  stroke="#0ea5e9" name="습도(%)" dot={false} />
          </LineChart>
        </ResponsiveContainer>
      </div>

      <h2>최근 샘플</h2>
      <table>
        <thead>
          <tr>
            <th>시각</th><th>모델</th><th>측정(μm)</th><th>목표(μm)</th>
            <th>현재(bar)</th><th>권장(bar)</th><th>T(°C)</th><th>H(%)</th>
          </tr>
        </thead>
        <tbody>
          {data.slice().reverse().slice(0, 20).map((r, i) => {
            const delta = r.recommended - r.current;
            return (
              <tr key={i}>
                <td>{r.t}</td>
                <td>{r.model_no}</td>
                <td>{r.measured_um.toFixed(1)}</td>
                <td>{r.target_um.toFixed(1)}</td>
                <td>{r.current.toFixed(2)}</td>
                <td className={Math.abs(delta) > 0.2 ? 'warn' : ''}>
                  {r.recommended.toFixed(2)} ({delta >= 0 ? '+' : ''}{delta.toFixed(2)})
                </td>
                <td>{r.temp.toFixed(1)}</td>
                <td>{r.hum.toFixed(0)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <h2>테스트 송신</h2>
      <p className="hint">
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
      {err && <p className="err">{err}</p>}
    </section>
  );
}
