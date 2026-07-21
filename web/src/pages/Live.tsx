import LivePanel from '../components/LivePanel';

// 실시간 카메라 영상 (전체 화면). 임베드는 LivePanel(go2rtc MSE) 재사용.
export default function Live() {
  return (
    <section>
      <h1>실시간 영상</h1>
      <p className="cam-sub">현대정밀 R1 도장라인 · 소재 인식 카메라</p>

      <LivePanel />

      <p className="cam-note">
        영상이 보이지 않으면 엣지 PC에서 카메라 스트림이 송출 중인지 확인하세요.
        (스냅샷은 뜨는데 영상만 끊기면 네트워크/방화벽 문제일 수 있습니다.)
      </p>
    </section>
  );
}
