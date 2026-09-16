import { Fragment, useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  api,
  saveRecipe,
  PlcRecipe,
  RecipeAxis,
  RobotCurrent,
  RobotEventDay,
} from '../lib/api';
import { MODEL_NOS, defaultRecipe, fitAxis } from '../lib/recipeDefaults';
import { EVENT_LABELS, label as symLabel } from '../lib/robotLabels';
import Blueprint from '../components/Blueprint';
import RobotChain from '../components/RobotChain';
import JigStrip from '../components/JigStrip';
import { BitGroup, BitLegend } from '../components/Bits';

// 이 화면은 "차종 N을 고르면 설비에서 무슨 일이 일어나는가"를 보여준다.
// `plc_r.md` §0이 그 답을 두 갈래로 적어 두었다:
//
//   HMI 차종번호 ─┬→ 레시피 → D/A 출력 → 도장건      (아래 레시피 표)
//                 └→ WORK ID → JIG 시프트 → 로봇 JOB  (아래 JIG·로봇 패널)
//
// 그래서 화면도 같은 순서로 갈라 놓았다. 두 갈래가 어긋나는 구간 —
// HMI는 새 차종인데 컨베이어 위 차체는 이전 차종인 구간 — 이 오도장이고,
// 그게 맨 위 경고 띠다.

type AxisKey = 'atomization' | 'pattern' | 'flow';
// 에어 스프레이 건 기준 용어. 리코일 스프링은 코일 형상이라 코일 사이가 깊고
// 그늘져, 넓고 부드럽게 뿌리는 회전식 벨보다 방향성 있는 건이 맞는다.
// 레시피에 전압(kV) 항목이 없는 것도 정전 벨이 아님을 뒷받침한다.
const AXES: { key: AxisKey; label: string; en: string; unit: string }[] = [
  { key: 'atomization', label: '무화', en: 'Atomizing Air', unit: '%' },
  { key: 'pattern', label: '패턴', en: 'Fan Air', unit: '%' },
  { key: 'flow', label: '토출량', en: 'Fluid Flow', unit: '%' },
];

function fmtDate(ms?: number | null) {
  if (!ms) return null;
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function fmtClock(ms?: number | null) {
  if (!ms) return null;
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/// 마지막 수신이 얼마나 지났나. 폴링이 멎으면 화면이 옛날 값을 현재처럼
/// 보여주게 되므로, 신선도를 항상 같이 적는다.
function staleness(ms?: number | null): { text: string; stale: boolean } | null {
  if (!ms) return null;
  const sec = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (sec < 60) return { text: `${sec}초 전`, stale: sec > 30 };
  if (sec < 3600) return { text: `${Math.round(sec / 60)}분 전`, stale: true };
  return { text: `${Math.round(sec / 3600)}시간 전`, stale: true };
}

type Alert = { tone: 'bad' | 'warn'; title: string; body: string };

/// 지금 눈에 띄어야 하는 것만 모은다. 없으면 띠를 그리지 않는다 —
/// "이상 없음" 배너는 매번 같은 자리에 떠서 아무도 읽지 않게 된다.
function alertsFor(r?: RobotCurrent): Alert[] {
  const out: Alert[] = [];
  const d = r?.derived;
  if (!r?.edge_id || !d) return out;

  if (d.work_id_mismatch === true) {
    out.push({
      tone: 'bad',
      title: '오도장 위험',
      body: `HMI 차종은 ${r.model_no ?? '?'}인데 JIG ${d.active_jig ?? '?'}에 실린 차체는 ${d.active_work_id ?? '?'}입니다. 지금 도장되면 레시피가 어긋납니다.`,
    });
  }
  if (d.faults.length) {
    out.push({
      tone: 'bad',
      title: '로봇 알람',
      body: d.faults.map(f => symLabel(f.split('.').pop() ?? f)).join(' · '),
    });
  }
  if (d.io_disagree.length) {
    out.push({
      tone: 'warn',
      title: '지령 ↔ 출력 불일치',
      body: `내부 릴레이와 실제 출력 접점이 ${d.io_streak}회 연속 어긋납니다 (${d.io_disagree
        .map(symLabel)
        .join(', ')}). 출력 모듈·배선을 확인하세요.`,
    });
  }
  if (r.read_errors?.length) {
    out.push({
      tone: 'warn',
      title: `PLC 읽기 실패 ${r.read_errors.length}건`,
      body:
        r.read_errors.length >= 5
          ? '전 블록 실패 — PLC 연결이 끊긴 상태입니다.'
          : `${r.read_errors.join(' / ')} — 해당 구간 비트는 "모름"으로 표시됩니다.`,
    });
  }
  if (d.jig_words_all_zero) {
    out.push({
      tone: 'warn',
      title: 'JIG 워드 전 구간 0',
      body:
        'JIG 시프트 레지스터(%DW7000·%DW7500 블록)가 설정값까지 전부 0입니다. ' +
        '체터링 방지거리·JOB 시작 임계값은 워크 유무와 무관한 설정값이라 0일 수 없습니다. ' +
        '라인이 비어서가 아니라 주소가 현재 래더와 어긋났을 수 있어, JIG 칸을 상태로 읽지 마세요.',
    });
  }
  if (d.model_no_out_of_range) {
    out.push({
      tone: 'warn',
      title: 'HMI 차종번호 범위 밖',
      body: `수신값 ${r.model_no ?? 'null'}. 미입력이거나 전환 중일 수 있습니다.`,
    });
  }
  return out;
}

export default function Recipe() {
  const q = useQuery<PlcRecipe[]>({
    queryKey: ['plc', 'recipe', 'list'],
    queryFn: api.recipeList,
    refetchInterval: 60_000,
  });

  // 로봇 인터페이스는 상태 스냅샷이라 자주 본다. 레시피는 차종을 바꿀 때만
  // 오므로 느리게 본다.
  const robot = useQuery<RobotCurrent>({
    queryKey: ['plc', 'robot', 'current'],
    queryFn: api.robotCurrent,
    refetchInterval: 5_000,
  });
  const events = useQuery<RobotEventDay>({
    queryKey: ['plc', 'robot', 'events'],
    queryFn: () => api.robotEvents(),
    refetchInterval: 15_000,
  });

  // 서버에 있는 것 + 없는 모델의 기본값을 합쳐 항상 8종을 보여준다.
  // 어느 쪽인지는 `received`로 구분해 화면에 표시한다.
  const models = useMemo(() => {
    const byNo = new Map((q.data ?? []).map(r => [r.model_no, r]));
    return MODEL_NOS.map(no => {
      // edge_id가 'mock'이면 화면 확인용으로 넣어둔 값이다. PLC 실수신분과
      // 섞이면 어느 것이 설비 값인지 알 수 없으므로 배지를 달리한다.
      const hit = byNo.get(no);
      const mock = hit?.edge_id === 'mock';
      return { no, received: !!hit && !mock, mock, data: hit ?? defaultRecipe(no) };
    });
  }, [q.data]);

  const [sel, setSel] = useState<number>(MODEL_NOS[0]);
  const current = models.find(m => m.no === sel) ?? models[0];

  const rc = robot.data;
  const linked = !!rc?.edge_id;
  const hmiModel = rc?.model_no ?? null;
  const fresh = staleness(rc?.received_at);
  const alerts = alertsFor(rc);

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
  const evList = events.data?.events ?? [];

  return (
    <>
      <div className="page-head">
        <h1 className="page-title">차종 도장 레시피</h1>
        <span className="page-note">
          {MODEL_NOS.length}종 중 PLC 수신 {received}종 · Mock {mockCount}종
          {linked && fresh && (
            <> · 로봇 {rc?.robot_model ?? 'MPX2600'} {fresh.text}</>
          )}
        </span>
      </div>

      {alerts.map(a => (
        <div key={a.title} className={`alert-band ${a.tone}`}>
          <strong>{a.title}</strong>
          <span>{a.body}</span>
        </div>
      ))}

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
                className={`model-row${m.no === sel ? ' on' : ''}${m.no === hmiModel ? ' hmi' : ''}`}
                onClick={() => setSel(m.no)}
              >
                <span className="model-no">{m.no}</span>
                <span className="model-name">{m.data.model_name ?? '—'}</span>
                <span className={`model-state${m.received ? ' recv' : ''}${m.mock ? ' mock' : ''}`}>
                  {m.no === hmiModel
                    ? 'HMI 선택중'
                    : m.received
                      ? fmtDate(m.data.received_at) ?? '수신'
                      : m.mock
                        ? `Mock · ${fmtDate(m.data.received_at) ?? ''}`
                        : '기본값'}
                </span>
              </button>
            ))}
          </div>
        </Blueprint>

        <div className="recipe-stack">
          <Blueprint
            title="지령 체인"
            right={
              <div className={`verdict ${linked ? (fresh?.stale ? 'warn' : 'ok') : 'idle'}`}>
                {linked ? `수신 ${fresh?.text ?? ''}` : '로봇 수신 없음'}
              </div>
            }
            foot={
              linked ? (
                <span>차종번호 하나가 도장건과 로봇 양쪽으로 갈라져 나간다</span>
              ) : (
                <span>
                  엣지가 <code>POST /api/v1/plc/robot</code> 로 보내기 시작하면 채워집니다
                </span>
              )
            }
          >
            <RobotChain robot={rc} recipeModel={current.no} />
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

          <div className="robot-row">
            <Blueprint
              title="JIG 시프트 5단"
              right={
                <div className="verdict idle">
                  완료 WORK ID {rc?.jig?.completed?.work_id ?? '—'}
                </div>
              }
              foot={
                <span>
                  칸의 숫자는 그 자리에 실린 차체의 차종이다 · 아래는 시프트 거리 / JOB 시작 임계값
                </span>
              }
            >
              <JigStrip
                stations={rc?.jig?.stations ?? []}
                activeJig={rc?.derived?.active_jig ?? null}
                hmiModel={hmiModel}
                suspect={rc?.derived?.jig_words_all_zero}
              />
            </Blueprint>

            <Blueprint
              title="오늘 이벤트"
              right={
                <div className="verdict idle">
                  {Object.values(events.data?.counts ?? {}).reduce((a, b) => a + b, 0)}건
                </div>
              }
              foot={<span>스냅샷이 아니라 직전과 비교해 나온 전이만 남긴다</span>}
            >
              {evList.length === 0 ? (
                <p className="hint">오늘 기록된 전이가 없습니다.</p>
              ) : (
                <ol className="ev-list">
                  {evList.slice(-40).reverse().map((e, i) => (
                    <li key={`${e.ts_ms}-${e.kind}-${i}`} className={e.alert ? 'alert' : ''}>
                      <span className="ev-time">{fmtClock(e.ts_ms)}</span>
                      <span className="ev-kind">{EVENT_LABELS[e.kind] ?? e.kind}</span>
                      <span className="ev-detail">{e.detail}</span>
                    </li>
                  ))}
                </ol>
              )}
            </Blueprint>
          </div>

          <Blueprint
            title="로봇 인터페이스 비트"
            right={<BitLegend />}
            foot={
              <span>
                <code>지령</code>은 PLC 내부 릴레이, <code>출력</code>은 실제 배선 접점이다 —
                이름이 같아도 다른 신호이고, 어긋나면 배선·출력 모듈 이상 신호다 ·
                P 영역 비상정지 접점은 NC(평상시 닫힘)일 수 있어 ON을 이상으로 단정하지 않는다
              </span>
            }
          >
            {!linked ? (
              <p className="hint">
                아직 수신된 스냅샷이 없습니다. 엣지에서{' '}
                <code>hdm_paint --post-robot --watch 2</code> 를 띄우면 채워집니다.
              </p>
            ) : (
              <div className="bit-groups">
                <BitGroup title="지령 (M 릴레이)" group="command" bits={rc?.robot?.command} />
                <BitGroup title="운전 상태" group="state" bits={rc?.robot?.state} />
                <BitGroup title="알람" group="alarm" bits={rc?.robot?.alarm} />
                <BitGroup title="출력 접점 (P)" group="do" bits={rc?.robot?.do} />
                <BitGroup title="입력 접점 (P)" group="di" bits={rc?.robot?.di} />
                <BitGroup title="JIG 시프트 공통" group="common" bits={rc?.jig?.common} />
              </div>
            )}
          </Blueprint>
        </div>
      </section>
    </>
  );
}
