import { useEffect, useState } from 'react';

// 실시간 카메라 임베드 (go2rtc MSE). 라인·정합 탭과 현황 탭이 공유.
// Cloudflare 터널은 UDP를 넘기지 못하므로 WebRTC 대신 MSE(WebSocket)로 서빙한다.
const CAM_BASE = 'https://cam.coreon.build';
const SRC = 'paint_cam';
const PROBE_MS = 15_000;
const PROBE_TIMEOUT_MS = 8_000;

type CamState = 'checking' | 'live' | 'offline';

/**
 * 카메라가 붙어 있는지 확인한다.
 *
 * 플레이어는 cam.coreon.build의 크로스오리진 iframe이라 내부 상태를 읽을 수
 * 없다. 대신 스냅샷 엔드포인트를 `<img>`로 찔러본다 — CORS 없이도 load/error를
 * 받을 수 있는 유일한 경로다.
 *
 * go2rtc는 소스가 등록돼 있지만 producer가 없으면 **200에 빈 본문**을 준다.
 * 없는 소스면 404. 두 경우 모두 디코딩에 실패해 onerror로 떨어지므로, 실제로
 * 프레임이 나올 때만 live가 된다.
 */
function useCamState(): CamState {
  const [state, setState] = useState<CamState>('checking');

  useEffect(() => {
    let cancelled = false;
    const probe = () => {
      const img = new Image();
      const timer = window.setTimeout(() => {
        img.onload = img.onerror = null;
        if (!cancelled) setState('offline');
      }, PROBE_TIMEOUT_MS);
      const settle = (next: CamState) => {
        window.clearTimeout(timer);
        if (!cancelled) setState(next);
      };
      img.onload = () => settle(img.naturalWidth > 0 ? 'live' : 'offline');
      img.onerror = () => settle('offline');
      // 캐시를 타면 끊긴 뒤에도 계속 live로 보인다.
      img.src = `${CAM_BASE}/api/frame.jpeg?src=${SRC}&t=${Date.now()}`;
    };

    probe();
    const id = window.setInterval(probe, PROBE_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  return state;
}

/// 시험 방송 화면. 방송 컬러바를 그대로 쓴다 — 실제 영상이 아니라는 걸
/// 한눈에 알리는 게 목적이라, 대시보드 톤과 어긋나는 채도가 오히려 맞다.
function TestPattern({ checking }: { checking: boolean }) {
  return (
    <div className="testcard" role="img" aria-label="카메라 신호 없음 — 시험 방송 화면">
      <div className="testcard-bars">
        {['#bfbfbf', '#bfbf00', '#00bfbf', '#00bf00', '#bf00bf', '#bf0000', '#0000bf'].map(c => (
          <i key={c} style={{ background: c }} />
        ))}
      </div>
      <div className="testcard-strip">
        {['#0000bf', '#131313', '#bf00bf', '#131313', '#00bfbf', '#131313', '#bfbfbf'].map((c, i) => (
          <i key={i} style={{ background: c }} />
        ))}
      </div>
      <div className="testcard-foot">
        <i style={{ background: '#00214c', flexGrow: 5 }} />
        <i style={{ background: '#ffffff', flexGrow: 5 }} />
        <i style={{ background: '#32006a', flexGrow: 5 }} />
        <i style={{ background: '#131313', flexGrow: 9 }} />
        <i style={{ background: '#070707', flexGrow: 1 }} />
        <i style={{ background: '#131313', flexGrow: 1 }} />
        <i style={{ background: '#1d1d1d', flexGrow: 1 }} />
        <i style={{ background: '#131313', flexGrow: 6 }} />
      </div>
      <div className="testcard-plate">
        <div className="testcard-title">시험 방송</div>
        <div className="testcard-sub">
          CAM-01 · 투입구 · {checking ? '연결 확인 중' : '신호 없음'}
        </div>
      </div>
    </div>
  );
}

export default function LivePanel({ compact = false }: { compact?: boolean }) {
  const cam = useCamState();
  const live = cam === 'live';

  return (
    <div className="cam-card">
      <div className="cam-frame">
        {live ? (
          <iframe
            className="cam-iframe"
            src={`${CAM_BASE}/stream.html?src=${SRC}&mode=mse`}
            title="도장라인 카메라 실시간"
            allow="autoplay; fullscreen"
            loading="lazy"
          />
        ) : (
          <TestPattern checking={cam === 'checking'} />
        )}
        <span className={`cam-badge${live ? '' : ' off'}`}>
          <span className="cam-dot" />
          {live ? 'LIVE' : cam === 'checking' ? '확인 중' : '연결 대기'}
        </span>
      </div>
      <div className="cam-meta">
        <span>CAM-01 · 투입구</span>
        {!compact && (
          <>
            <span className="sep">·</span>
            <span>{live ? 'MSE · 저지연 ~0.5초' : '카메라 미연결'}</span>
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
