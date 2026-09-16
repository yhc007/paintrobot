import type { RobotCurrent } from '../lib/api';
import { state } from './Bits';

// `plc_r.md` §0의 지령 체인을 살아있는 그림으로 옮긴 것.
//
//   HMI 차종번호(%DW5000)
//      ├─→ 레시피 로드 → D/A 출력 → 도장건
//      └─→ JIG 시프트 WORK ID → 로봇: 차종별 JOB 실행
//
// 이 갈래가 이 화면의 뼈대다. 왼쪽 가지가 레시피 표, 오른쪽 가지가 JIG·로봇
// 패널이고, 두 가지가 같은 차종번호에서 갈라진다는 것이 요점이다.
// 두 가지가 어긋나면(HMI는 새 차종, 컨베이어 위 차체는 이전 차종) 오도장이다.

type Props = { robot?: RobotCurrent; recipeModel: number };

function Node({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <div className={`chain-node${tone ? ` ${tone}` : ''}`}>
      <span className="chain-label">{label}</span>
      <span className="chain-value">{value}</span>
    </div>
  );
}

export default function RobotChain({ robot, recipeModel }: Props) {
  const d = robot?.derived;
  const hmi = robot?.model_no ?? null;
  const jig = d?.active_jig ?? null;
  const workId = d?.active_work_id ?? null;
  const mismatch = d?.work_id_mismatch ?? null;
  const running = state(robot?.robot?.di?.job_in_progress ?? null);

  // 로봇 수신이 아직 없으면 왼쪽 가지만 사실이다. 오른쪽은 비워 두고
  // 지어내지 않는다.
  const linked = !!robot?.edge_id;

  return (
    <div className="chain">
      <Node
        label="HMI 차종번호"
        value={linked && hmi !== null ? String(hmi) : `${recipeModel} · 화면 선택`}
        tone={linked && hmi !== null ? 'live' : 'idle'}
      />
      <div className="chain-fork" aria-hidden="true" />
      <div className="chain-arms">
        <div className="chain-arm">
          <span className="chain-arm-tag">레시피</span>
          <Node label="D/A 출력" value="도장건 직결" tone="idle" />
          <p className="chain-note">
            PLC가 무화 · 패턴 · 토출량을 D/A로 직접 물린다. 이 값은 로봇으로 가지 않는다.
          </p>
        </div>
        <div className="chain-arm">
          <span className="chain-arm-tag">WORK ID</span>
          <Node
            label={jig ? `JIG ${jig} 차체` : 'JIG'}
            value={linked ? (workId !== null ? String(workId) : '—') : '수신 없음'}
            tone={mismatch === true ? 'bad' : mismatch === false ? 'live' : 'idle'}
          />
          <Node
            label="로봇 MPX2600"
            value={
              !linked
                ? '수신 없음'
                : running === 'on'
                  ? 'JOB 수행중'
                  : running === 'off'
                    ? '대기'
                    : '모름'
            }
            tone={running === 'on' ? 'live' : 'idle'}
          />
          <p className="chain-note">
            로봇은 WORK ID로 내부에 티칭된 차종별 JOB을 스스로 고른다.
          </p>
        </div>
      </div>
    </div>
  );
}
