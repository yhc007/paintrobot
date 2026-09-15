import { useEffect, useState } from 'react';

// 실시간 카메라 임베드 (go2rtc MSE). 라인·정합 탭과 현황 탭이 공유.
// Cloudflare 터널은 UDP를 넘기지 못하므로 WebRTC 대신 MSE(WebSocket)로 서빙한다.
//
// 플레이어는 cam 도메인의 크로스오리진 iframe이다. 페이지 안에서 직접 재생하는
// 방법도 시도했지만, go2rtc가 교차 출처 WebSocket을 403으로 막기 때문에
// (Origin이 cam 도메인과 달라진다) 이 서버 설정에서는 iframe이 유일한 경로다.
const CAM_BASE = 'https://cam.coreon.build';
const SRC = 'paint_cam';
const PROBE_MS = 15_000;
// 신호가 없을 때는 더 자주 확인한다. 카메라가 돌아오면 iframe 속 플레이어는
// 스스로 금방 재생을 되찾는데, 프로브가 느리면 살아난 영상 위에 시험 방송
// 화면이 한동안 얹혀 있게 된다.
const PROBE_OFFLINE_MS = 3_000;
const PROBE_TIMEOUT_MS = 8_000;

type CamState = 'checking' | 'live' | 'offline';

/**
 * 카메라에서 프레임이 나오는지 확인한다.
 *
 * 예전에는 스냅샷(`/api/frame.jpeg`)을 찔렀는데, 그 엔드포인트는 go2rtc가
 * ffmpeg를 호출하고 이 서버에는 ffmpeg가 없어 항상 500이었다. 그래서 멀쩡한
 * 스트림이 영원히 "미연결"로 표시됐다. HLS 플레이리스트는 ffmpeg 없이 go2rtc가
 * 직접 만들어 주고, CORS도 열려 있어 상태 코드와 본문을 그대로 읽을 수 있다.
 *
 *   200 + 본문 있음  → 송출 중
 *   200 + 빈 본문    → 스트림은 등록됐지만 카메라가 붙어 있지 않음
 *   404             → 그런 스트림이 없음
 *   그 외/예외       → 판정 보류. 프로브가 고장 났을 뿐 카메라 문제라는 근거가 아니다.
 */
async function probeOnce(signal: AbortSignal): Promise<CamState> {
  try {
    const res = await fetch(`${CAM_BASE}/api/stream.m3u8?src=${SRC}`, {
      cache: 'no-store',
      signal,
    });
    if (res.status === 404) return 'offline';
    if (!res.ok) return 'checking';
    const body = await res.text();
    return body.trim().length > 0 ? 'live' : 'offline';
  } catch {
    // 네트워크 오류·타임아웃·CORS 실패 등. 영상을 가리지 않는다.
    return 'checking';
  }
}

function useCamState(): CamState {
  const [state, setState] = useState<CamState>('checking');

  useEffect(() => {
    let cancelled = false;
    let nextTick = 0;

    // 고정 간격(setInterval) 대신 매번 다음 주기를 정한다 — 상태에 따라 간격이 다르다.
    const tick = async () => {
      const ctrl = new AbortController();
      const timer = window.setTimeout(() => ctrl.abort(), PROBE_TIMEOUT_MS);
      const next = await probeOnce(ctrl.signal);
      window.clearTimeout(timer);
      if (cancelled) return;

      setState(next);
      nextTick = window.setTimeout(
        () => void tick(),
        next === 'offline' ? PROBE_OFFLINE_MS : PROBE_MS,
      );
    };

    void tick();
    return () => {
      cancelled = true;
      window.clearTimeout(nextTick);
    };
  }, []);

  return state;
}

/// 시험 방송 화면. 방송 컬러바를 그대로 쓴다 — 실제 영상이 아니라는 걸
/// 한눈에 알리는 게 목적이라, 대시보드 톤과 어긋나는 채도가 오히려 맞다.
function TestPattern() {
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
        <div className="testcard-sub">CAM-01 · 투입구 · 신호 없음</div>
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
        {/* 플레이어는 언제나 붙어 있다. 프로브 결과로 이걸 떼어내면, 프로브 하나가
            고장났을 때 멀쩡한 영상까지 사라진다 — 실제로 그렇게 고장나 있었다. */}
        <iframe
          className="cam-iframe"
          src={`${CAM_BASE}/stream.html?src=${SRC}&mode=mse`}
          title="도장라인 카메라 실시간"
          allow="autoplay; fullscreen"
          loading="lazy"
        />
        {/* 시험 방송 화면은 "카메라가 없다"는 확실한 근거가 있을 때만 덮는다. */}
        {cam === 'offline' && <TestPattern />}
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
          </>
        )}
        <span className="sep">·</span>
        <a href={`${CAM_BASE}/stream.html?src=${SRC}`} target="_blank" rel="noreferrer">
          전체화면
        </a>
      </div>

      {/* 안내는 신호가 없을 때만. 잘 나오는 카메라 밑에 문제 해결 안내를
          상시로 띄워두면 읽히지 않는 문구가 된다. */}
      {!compact && cam === 'offline' && (
        <p className="hint">
          카메라에서 프레임이 오지 않습니다. 엣지 PC에서 스트림이 송출 중인지
          확인하세요. 몇 초마다 다시 확인하며, 연결되면 곧바로 영상으로 바뀝니다.
        </p>
      )}
    </div>
  );
}
