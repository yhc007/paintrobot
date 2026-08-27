// 실시간 카메라 임베드 (go2rtc MSE). 라인·정합 탭과 현황 탭이 공유.
// Cloudflare 터널은 UDP를 넘기지 못하므로 WebRTC 대신 MSE(WebSocket)로 서빙한다.
const CAM_BASE = 'https://cam.coreon.build';
const SRC = 'paint_cam';

export default function LivePanel({ compact = false }: { compact?: boolean }) {
  return (
    <div className="cam-card">
      <div className="cam-frame">
        <iframe
          className="cam-iframe"
          src={`${CAM_BASE}/stream.html?src=${SRC}&mode=mse`}
          title="도장라인 카메라 실시간"
          allow="autoplay; fullscreen"
          loading="lazy"
        />
        <span className="cam-badge"><span className="cam-dot" /> LIVE</span>
      </div>
      <div className="cam-meta">
        <span>CAM-01 · 투입구</span>
        {!compact && (
          <>
            <span className="sep">·</span>
            <span>MSE · 저지연 ~0.5초</span>
            <span className="sep">·</span>
            <a href={`${CAM_BASE}/api/frame.jpeg?src=${SRC}`} target="_blank" rel="noreferrer">
              스냅샷
            </a>
          </>
        )}
        <span className="sep">·</span>
        <a href={`${CAM_BASE}/stream.html?src=${SRC}`} target="_blank" rel="noreferrer">
          전체화면
        </a>
      </div>
    </div>
  );
}
