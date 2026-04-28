import type { PlcCurrent } from '../lib/api';

function fmtKstTime(ms: number | null): string {
  if (!ms) return '';
  const d = new Date(ms);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

function freshness(ms: number | null): { label: string; stale: boolean } {
  if (!ms) return { label: '-', stale: true };
  const ageSec = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (ageSec < 60) return { label: `${ageSec}초 전`, stale: false };
  const m = Math.floor(ageSec / 60);
  if (m < 60) return { label: `${m}분 전`, stale: m >= 5 };
  const h = Math.floor(m / 60);
  return { label: `${h}시간 전`, stale: true };
}

export default function PlcCard({ plc }: { plc: PlcCurrent | null | undefined }) {
  const has = !!plc?.model_no;
  const fresh = freshness(plc?.plc_ts ?? null);
  return (
    <div className={`plc-card ${has ? '' : 'plc-empty'}`}>
      <div className="plc-label">현재 PLC 모델</div>
      <div className="plc-model">{has ? plc!.model_no : '— 대기중 —'}</div>
      <div className="plc-meta">
        <span className={`plc-fresh ${fresh.stale ? 'stale' : ''}`}>
          {has ? fresh.label : '신호 없음'}
        </span>
        {has && plc?.edge_id && (
          <>
            <span className="dot">·</span>
            <span>{plc.edge_id}</span>
          </>
        )}
        {has && plc?.plc_ts && (
          <>
            <span className="dot">·</span>
            <span>{fmtKstTime(plc.plc_ts)}</span>
          </>
        )}
      </div>
    </div>
  );
}
