import type { PlcCurrent } from '../lib/api';
import Blueprint from './Blueprint';

function fmtKstTime(ms: number | null): string {
  if (!ms) return '-';
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export function freshness(ms: number | null): { label: string; stale: boolean } {
  if (!ms) return { label: '-', stale: true };
  const ageSec = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (ageSec < 60) return { label: `${ageSec}초 전`, stale: false };
  const m = Math.floor(ageSec / 60);
  if (m < 60) return { label: `${m}분 전`, stale: m >= 5 };
  return { label: `${Math.floor(m / 60)}시간 전`, stale: true };
}

export type MatchState = 'match' | 'mismatch' | 'waiting' | 'none';

export function matchState(plc: PlcCurrent | null | undefined): MatchState {
  const p = plc?.model_no;
  const c = plc?.camera_model_no;
  if (!p && !c) return 'none';
  if (!p || !c) return 'waiting';
  return p === c ? 'match' : 'mismatch';
}

const VERDICT: Record<MatchState, { label: string; cls: string }> = {
  match: { label: '일치', cls: 'ok' },
  mismatch: { label: '불일치', cls: 'bad' },
  waiting: { label: '한쪽 대기', cls: 'warn' },
  none: { label: '신호 없음', cls: 'idle' },
};

// 모델 정합 패널 — PLC 지시값과 카메라 인식값을 나란히 놓고 일치 여부를 판정.
export default function PlcCard({ plc }: { plc: PlcCurrent | null | undefined }) {
  const state = matchState(plc);
  const v = VERDICT[state];
  const plcFresh = freshness(plc?.plc_ts ?? null);
  const camFresh = freshness(plc?.camera_ts ?? null);
  const eqSign = state === 'match' ? '=' : state === 'mismatch' ? '≠' : '·';

  return (
    <Blueprint
      title="모델 정합 · PLC ↔ 카메라"
      right={<div className={`verdict ${v.cls}`}>{v.label}</div>}
      foot={
        <>
          <span className={plcFresh.stale ? 'warn' : undefined}>PLC 수신 {plcFresh.label}</span>
          <span className={camFresh.stale ? 'warn' : undefined}>CAM 수신 {camFresh.label}</span>
          {plc?.edge_id && <span className="push">{plc.edge_id}</span>}
        </>
      }
    >
      <div className="match-row">
        <div className="match-side">
          <div className="gauge-label">PLC 지시</div>
          <div className={`gauge-value${plc?.model_no ? '' : ' idle'}`}>{plc?.model_no ?? '—'}</div>
        </div>
        <div className={`match-eq${state === 'mismatch' ? ' bad' : ''}`}>{eqSign}</div>
        <div className="match-side">
          <div className="gauge-label">카메라 인식</div>
          <div className={`gauge-value${plc?.camera_model_no ? '' : ' idle'}`}>
            {plc?.camera_model_no ?? '—'}
          </div>
        </div>
        <div className="match-aside">
          <div>PLC {fmtKstTime(plc?.plc_ts ?? null)}</div>
          <div>CAM {fmtKstTime(plc?.camera_ts ?? null)}</div>
        </div>
      </div>
    </Blueprint>
  );
}
