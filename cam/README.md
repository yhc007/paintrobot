# 실시간 카메라 스트리밍 (go2rtc)

도장라인 소재 인식 카메라 영상을 웹 대시보드(`https://paint.coreon.build/live`)에서
실시간으로 본다. 전송 지연 **~0.5초 (MSE)**.

```
[Edge PC: 소재판별 앱]                       [root1 서버]                       [브라우저]
 카메라 프레임(추론용 디코드)
      │ 같은 프레임 재사용
      ▼
   ffmpeg (H.264 인코딩)
      │  RTSP push (LAN, TCP)
      │   rtsp://192.168.10.30:8554/paint_cam
      ▼
   go2rtc :8554 수신 ──▶ :1984 (api/MSE) ──cloudflared──▶ cam.coreon.build
                                                              │  MSE / WSS
                                        paint.coreon.build/live 에 iframe 임베드 ◀┘
```

- USB/GigE 카메라는 두 프로세스가 동시에 못 여므로, **카메라를 이미 쥔 앱이 프레임을 재사용**해
  ffmpeg로 넘기는 것이 유일하게 견고한 방식이다. 별도 프로그램이 카메라를 직접 열려 하면 안 된다.

---

## root1 서버 (구축 완료 — 참고용)

| 구성 | 값 |
|---|---|
| go2rtc 바이너리 | `~/.local/bin/go2rtc` (v1.9.14) |
| 설정 | `cam/go2rtc.yaml` |
| systemd 유닛 | `paintrobot-cam.service` (user, linger) |
| RTSP 수신 | `:8554` (전 인터페이스, Edge가 여기로 push) |
| API/MSE | `127.0.0.1:1984` (cloudflared 터널만 프록시) |
| 공개 URL | `https://cam.coreon.build` — **시청 전용 경로만** 열림 |
| 스트림 이름 | `paint_cam` |

```bash
systemctl --user status paintrobot-cam         # 상태
journalctl --user -u paintrobot-cam -f          # 로그(systemd)
tail -f logs/go2rtc.log                          # 로그(파일)
curl http://127.0.0.1:1984/api/streams           # 수신 중인 스트림 확인
```

> 보안: `cam.coreon.build`는 `stream.html`·`video-stream.js`·`video-rtc.js`·`/api/ws`·
> `/api/frame.jpeg`·`/api/stream.*` 만 노출한다. go2rtc의 스트림 추가/설정/재시작 API는
> 터널에서 404로 차단(`cloudflared.yml` 참조) — exec 소스를 통한 RCE 방지.

---

## Edge PC 설정 (여기만 하면 됨)

### 1. ffmpeg 설치

- Ubuntu/Debian: `sudo apt-get install -y ffmpeg`
- 그 외 Linux: static build (https://johnvansickle.com/ffmpeg/ 또는 https://github.com/BtbN/FFmpeg-Builds)
- Windows: https://www.gyan.dev/ffmpeg/builds/ 의 `ffmpeg.exe`

### 2. 앱에서 프레임을 ffmpeg로 push (권장)

소재판별 앱은 이미 추론용으로 프레임(`frame`, BGR `numpy.ndarray`)을 갖고 있다.
그 프레임을 **그대로** ffmpeg stdin에 흘려보내면 된다 — 카메라를 두 번 열지 않는다.

```python
import subprocess

RTSP_URL = "rtsp://192.168.10.30:8554/paint_cam"  # root1 (사내망 IP)
W, H, FPS = 1280, 720, 15                          # 앱의 실제 프레임 크기/속도에 맞출 것

_ff = subprocess.Popen(
    [
        "ffmpeg", "-hide_banner", "-loglevel", "warning",
        "-f", "rawvideo", "-pix_fmt", "bgr24",     # OpenCV BGR 프레임
        "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
        "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency",
        "-pix_fmt", "yuv420p", "-g", str(FPS * 2), "-bf", "0",
        "-f", "rtsp", "-rtsp_transport", "tcp", RTSP_URL,
    ],
    stdin=subprocess.PIPE,
)

def publish_frame(frame):
    """소재판별 추론 루프에서 프레임마다 호출 (frame: HxWx3 BGR)."""
    import cv2
    if frame.shape[1] != W or frame.shape[0] != H:
        frame = cv2.resize(frame, (W, H))
    try:
        _ff.stdin.write(frame.tobytes())
    except BrokenPipeError:
        pass  # ffmpeg 재시작 로직은 운영 상황에 맞게 추가

# 예: 기존 추론 루프
#   ok, frame = cap.read()
#   result = classify(frame)   # 기존 소재판별
#   publish_frame(frame)       # ← 이 한 줄 추가
```

### 3. (대안) 카메라/앱이 이미 RTSP/RTMP를 내보내는 경우

앱을 건드리지 않고 ffmpeg로 릴레이만 하면 된다(재인코딩 없이 copy):

```bash
ffmpeg -rtsp_transport tcp -i rtsp://<카메라주소>/stream \
  -c copy -f rtsp -rtsp_transport tcp rtsp://192.168.10.30:8554/paint_cam
```

### 4. 확인

```bash
# 스냅샷 1장 (송출이 되면 JPEG가 저장됨)
curl -o test.jpg "https://cam.coreon.build/api/frame.jpeg?src=paint_cam"
```

브라우저에서 **https://paint.coreon.build/live** → 영상이 뜨면 완료.

---

## 튜닝 메모

- **지연 최소화**: `-tune zerolatency`, `-bf 0`, GOP(`-g`)를 FPS의 1~2배로. 이미 반영됨.
- **해상도/FPS**: 앱 프레임에 맞춰 `W/H/FPS` 만 바꾸면 된다. 과한 해상도는 대역폭만 먹는다.
- **여러 라인**: 스트림 이름을 `paint_cam_02` 등으로 나누고 `go2rtc.yaml` streams에 키 추가 후
  `paintrobot-cam` 재시작. SPA `Live.tsx`의 `SRC`도 맞춰 변경.
