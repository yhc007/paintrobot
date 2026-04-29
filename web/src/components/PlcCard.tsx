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
  const hasPlc = !!plc?.model_no;
  const hasCam = !!plc?.camera_model_no;
  const fresh = freshness(plc?.plc_ts ?? null);
  const camFresh = freshness(plc?.camera_ts ?? null);
  return (
    <div className={`plc-card ${hasPlc ? '' : 'plc-empty'}`}>
      <div className="plc-label">현재 PLC 모델 · 카메라 인식</div>
      <div className="plc-model">
        {hasPlc ? plc!.model_no : '— 대기중 —'}
        {hasCam && (
          <>
            <span className="plc-sep"> - </span>
            <span className="plc-camera">{plc!.camera_model_no}</span>
          </>
        )}
      </div>
      <div className="plc-meta">
        {hasPlc && (
          <>
            <span className={`plc-fresh ${fresh.stale ? 'stale' : ''}`}>
              PLC {fresh.label}
            </span>
          </>
        )}
        {hasCam && (
          <span className={`plc-fresh cam ${camFresh.stale ? 'stale' : ''}`}>
            CAM {camFresh.label}
          </span>
        )}
        {!hasPlc && !hasCam && <span className="plc-fresh stale">신호 없음</span>}
        {plc?.edge_id && (
          <>
            <span className="dot">·</span>
            <span>{plc.edge_id}</span>
          </>
        )}
        {hasPlc && plc?.plc_ts && (
          <>
            <span className="dot">·</span>
            <span>PLC {fmtKstTime(plc.plc_ts)}</span>
          </>
        )}
        {hasCam && plc?.camera_ts && (
          <>
            <span className="dot">·</span>
            <span>CAM {fmtKstTime(plc.camera_ts)}</span>
          </>
        )}
      </div>
    </div>
  );
}
