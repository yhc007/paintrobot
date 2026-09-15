import { Fragment, useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api, saveRecipe, PlcRecipe, RecipeAxis } from '../lib/api';
import { MODEL_NOS, defaultRecipe, fitAxis } from '../lib/recipeDefaults';
import Blueprint from '../components/Blueprint';

type AxisKey = 'atomization' | 'pattern' | 'flow';
const AXES: { key: AxisKey; label: string; en: string; unit: string }[] = [
  { key: 'atomization', label: '무화', en: 'Atomization', unit: '%' },
  { key: 'pattern', label: '패턴', en: 'Pattern', unit: '%' },
  { key: 'flow', label: '토출량', en: 'Flow', unit: '%' },
];

function fmtTime(ms?: number | null) {
  if (!ms) return null;
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export default function Recipe() {
  const q = useQuery<PlcRecipe[]>({
    queryKey: ['plc', 'recipe', 'list'],
    queryFn: api.recipeList,
    refetchInterval: 60_000,
  });

  // 서버에 있는 것 + 없는 모델의 기본값을 합쳐 항상 8종을 보여준다.
  // 어느 쪽인지는 `received`로 구분해 화면에 표시한다.
  const models = useMemo(() => {
    const byNo = new Map((q.data ?? []).map(r => [r.model_no, r]));
    return MODEL_NOS.map(no => {
      const hit = byNo.get(no);
      // edge_id가 'mock'이면 화면 확인용으로 넣어둔 값이다. PLC 실수신분과
      // 섞이면 어느 것이 설비 값인지 알 수 없으므로 배지를 달리한다.
      const mock = hit?.edge_id === 'mock';
      return { no, received: !!hit && !mock, mock, data: hit ?? defaultRecipe(no) };
    });
  }, [q.data]);

  const [sel, setSel] = useState<number>(MODEL_NOS[0]);
  const current = models.find(m => m.no === sel) ?? models[0];

  // 편집 버퍼. 모델을 바꾸거나 서버 데이터가 갱신되면 다시 채운다.
  const [draft, setDraft] = useState<Record<AxisKey, RecipeAxis>>(() => blank());
  const [levels, setLevels] = useState(8);
  const [name, setName] = useState('');
  const [dirty, setDirty] = useState(false);

  function blank(): Record<AxisKey, RecipeAxis> {
    const z = { table: [], applied: [] } as RecipeAxis;
    return { atomization: z, pattern: z, flow: z };
  }

  useEffect(() => {
    if (!current) return;
    const lv = current.data.levels ?? 8;
    setLevels(lv);
    setName(current.data.model_name ?? '');
    const r = current.data.recipe;
    setDraft({
      atomization: fitAxis(r?.atomization, lv),
      pattern: fitAxis(r?.pattern, lv),
      flow: fitAxis(r?.flow, lv),
    });
    setDirty(false);
  }, [sel, q.dataUpdatedAt]);

  const [edgeKey, setEdgeKey] = useState(() => localStorage.getItem('edgeKey') ?? '');
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null);

  const setCell = (axis: AxisKey, field: 'table' | 'applied', i: number, v: number) => {
    setDraft(d => {
      const next = { ...d, [axis]: { ...d[axis], [field]: [...d[axis][field]] } };
      next[axis][field][i] = v;
      return next;
    });
    setDirty(true);
  };

  const onSave = async () => {
    setBusy(true);
    setMsg(null);
    try {
      localStorage.setItem('edgeKey', edgeKey);
      await saveRecipe(
        {
          edge_id: current.data.edge_id ?? 'edge-line-01',
          model_no: current.no,
          model_name: name.trim() || String(current.no),
          levels,
          recipe: draft,
        },
        edgeKey,
      );
      setMsg({ kind: 'ok', text: `모델 ${current.no} 저장했습니다.` });
      setDirty(false);
      q.refetch();
    } catch (e) {
      setMsg({ kind: 'err', text: String(e instanceof Error ? e.message : e) });
    } finally {
      setBusy(false);
    }
  };

  const received = models.filter(m => m.received).length;
  const mockCount = models.filter(m => m.mock).length;

  return (
    <>
      <div className="page-head">
        <h1 className="page-title">차종 도장 레시피</h1>
        <span className="page-note">
          {MODEL_NOS.length}종 중 PLC 수신 {received}종 · Mock {mockCount}종
        </span>
      </div>

      {q.isLoading && <p className="hint">로딩중…</p>}
      {q.error && <p className="err">{String(q.error)}</p>}

      <section className="recipe-split">
        <Blueprint
          title="차종"
          foot={<span>Mock은 화면 확인용 값이며 설비에서 온 것이 아닙니다</span>}
        >
          <div className="model-list">
            {models.map(m => (
              <button
                key={m.no}
                type="button"
                className={`model-row${m.no === sel ? ' on' : ''}`}
                onClick={() => setSel(m.no)}
              >
                <span className="model-no">{m.no}</span>
                <span className="model-name">{m.data.model_name ?? '—'}</span>
                <span className={`model-state${m.received ? ' recv' : ''}${m.mock ? ' mock' : ''}`}>
                  {m.received
                    ? fmtTime(m.data.received_at) ?? '수신'
                    : m.mock
                      ? `Mock · ${fmtTime(m.data.received_at) ?? ''}`
                      : '기본값'}
                </span>
              </button>
            ))}
          </div>
        </Blueprint>

        <Blueprint
          title={`모델 ${current.no} · 레시피`}
          right={
            <div className={`verdict ${current.received ? 'ok' : current.mock ? 'warn' : 'idle'}`}>
              {current.received ? 'PLC 수신' : current.mock ? 'Mock 데이터' : '기본값'}
            </div>
          }
          foot={
            <>
              <span>적용값이 저장값과 다르면 주황</span>
              {dirty && <span className="push warn">수정됨 · 저장 전</span>}
            </>
          }
        >
          <div className="recipe-meta">
            <label>
              차종명
              <input
                value={name}
                placeholder="예: 140"
                onChange={e => { setName(e.target.value); setDirty(true); }}
              />
            </label>
            <label>
              단계 수
              <input
                type="number" min={1} max={16} value={levels}
                onChange={e => {
                  const lv = Math.max(1, Math.min(16, Number(e.target.value) || 1));
                  setLevels(lv);
                  setDraft(d => ({
                    atomization: fitAxis(d.atomization, lv),
                    pattern: fitAxis(d.pattern, lv),
                    flow: fitAxis(d.flow, lv),
                  }));
                  setDirty(true);
                }}
              />
            </label>
          </div>

          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th rowSpan={2}>단계</th>
                  {AXES.map(a => (
                    <th key={a.key} colSpan={2} style={{ textAlign: 'center' }}>
                      {a.label} · {a.unit}
                      <span className="axis-en">{a.en}</span>
                    </th>
                  ))}
                </tr>
                <tr>
                  {AXES.map(a => (
                    <Fragment key={a.key}>
                      <th style={{ textAlign: 'right' }}>저장</th>
                      <th style={{ textAlign: 'right' }}>적용</th>
                    </Fragment>
                  ))}
                </tr>
              </thead>
              <tbody>
                {Array.from({ length: levels }, (_, i) => (
                  <tr key={i}>
                    <td><span className="model">{i + 1}</span></td>
                    {AXES.map(a => {
                      const t = draft[a.key].table[i] ?? 0;
                      const ap = draft[a.key].applied[i] ?? 0;
                      return (
                        <Fragment key={a.key}>
                          <td className="num">
                            <input
                              className="cell" type="number" value={t}
                              onChange={e => setCell(a.key, 'table', i, Number(e.target.value) || 0)}
                            />
                          </td>
                          <td className="num">
                            <input
                              className={`cell${t !== ap ? ' warn' : ''}`} type="number" value={ap}
                              onChange={e => setCell(a.key, 'applied', i, Number(e.target.value) || 0)}
                            />
                          </td>
                        </Fragment>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="recipe-save">
            <label>
              엣지 키
              <input
                type="password" value={edgeKey} placeholder="쓰기 권한 키"
                onChange={e => setEdgeKey(e.target.value)}
              />
            </label>
            <button type="button" onClick={onSave} disabled={busy || !edgeKey || !dirty}>
              {busy ? '저장중…' : '저장'}
            </button>
            {msg && <span className={msg.kind === 'ok' ? 'ok' : 'err'}>{msg.text}</span>}
          </div>
          <p className="hint">
            저장하면 PLC 수신과 같은 자리에 기록됩니다. 이후 PLC가 실제 값을 보내오면
            그 값이 덮어씁니다.
          </p>
        </Blueprint>
      </section>
    </>
  );
}
