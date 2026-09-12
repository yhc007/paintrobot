import { Fragment } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api, PlcRecipe, RecipeAxis } from '../lib/api';
import Blueprint from '../components/Blueprint';

// 레시피의 세 축. PLC가 보내는 키 이름과 화면 라벨을 한 곳에 묶어둔다.
const AXES: { key: keyof NonNullable<PlcRecipe['recipe']>; label: string; unit: string }[] = [
  { key: 'atomization', label: '무화', unit: '%' },
  { key: 'pattern', label: '패턴', unit: '%' },
  { key: 'flow', label: '토출량', unit: '%' },
];

function fmtTime(ms?: number | null) {
  if (!ms) return '—';
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/// 저장값과 적용값이 다르면 현장에서 손을 댔다는 뜻이라 눈에 띄어야 한다.
function Cell({ table, applied }: { table?: number; applied?: number }) {
  const t = table ?? 0;
  const a = applied ?? 0;
  const differs = t !== a;
  return (
    <>
      <td className="num">{t}</td>
      <td className={`num${differs ? ' warn' : ''}`}>{a}</td>
    </>
  );
}

export default function Recipe() {
  const q = useQuery<PlcRecipe>({
    queryKey: ['plc', 'recipe', 'current'],
    queryFn: api.recipeCurrent,
    refetchInterval: 30_000,
  });

  const r = q.data;
  const recipe = r?.recipe ?? null;
  // levels가 없으면 실제 배열 길이로 대신한다 — 둘이 어긋난 데이터가 와도
  // 표가 깨지지 않게.
  const levels =
    r?.levels ??
    (recipe ? Math.max(...AXES.map(a => (recipe[a.key] as RecipeAxis).table.length)) : 0);

  const axisOf = (k: (typeof AXES)[number]['key']): RecipeAxis =>
    (recipe?.[k] as RecipeAxis) ?? { table: [], applied: [] };

  return (
    <>
      <div className="page-head">
        <h1 className="page-title">차종 도장 레시피</h1>
        <span className="page-note">
          {r?.edge_id ?? 'edge'} · 30초 주기 갱신 · PLC 수신값
        </span>
      </div>

      {q.isLoading && <p className="hint">로딩중…</p>}
      {q.error && <p className="err">{String(q.error)}</p>}

      {q.isSuccess && !recipe && (
        <Blueprint title="현재 레시피">
          <div className="hint">
            아직 수신된 레시피가 없습니다. PLC가 차종을 바꾸면 이 자리에 표시됩니다.
          </div>
        </Blueprint>
      )}

      {recipe && (
        <>
          <section className="row-3">
            <Blueprint
              title="현재 차종"
              right={<div className="verdict idle">모델 {r?.model_no ?? '—'}</div>}
              foot={<span>수신 {fmtTime(r?.received_at)}</span>}
            >
              <div className="match-row">
                <div className="gauge-value">{r?.model_name ?? '—'}</div>
              </div>
            </Blueprint>

            <Blueprint title="단계 수" foot={<span>레시피 배열 길이</span>}>
              <div className="match-row">
                <div className="gauge-value ice">
                  {levels}
                  <span className="gauge-unit"> 단</span>
                </div>
              </div>
            </Blueprint>

            <Blueprint title="기준일" foot={<span>PLC 수신 기준</span>}>
              <div className="match-row">
                <div className="gauge-value mid">{r?.work_date ?? '—'}</div>
              </div>
            </Blueprint>
          </section>

          <Blueprint
            title="레시피 상세 · 단계별"
            right={
              <div className="legend">
                <span>저장 = PLC 테이블값</span>
                <span>적용 = 실제 적용값</span>
              </div>
            }
            foot={<span>적용값이 저장값과 다르면 주황으로 표시됩니다</span>}
          >
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th rowSpan={2}>단계</th>
                    {AXES.map(a => (
                      <th key={a.key} colSpan={2} style={{ textAlign: 'center' }}>
                        {a.label} · {a.unit}
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
                      {AXES.map(a => (
                        <Cell
                          key={a.key}
                          table={axisOf(a.key).table[i]}
                          applied={axisOf(a.key).applied[i]}
                        />
                      ))}
                    </tr>
                  ))}
                  {levels === 0 && (
                    <tr><td className="empty-row" colSpan={7}>단계 데이터 없음</td></tr>
                  )}
                </tbody>
              </table>
            </div>
          </Blueprint>
        </>
      )}
    </>
  );
}
