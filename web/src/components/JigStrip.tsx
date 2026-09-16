import type { JigStation } from '../lib/api';
import { state } from './Bits';

// 컨베이어 JIG 5단 시프트 레지스터.
//
// 차체가 실리면 그 칸의 `work_id`에 차종번호가 박히고, 컨베이어가 움직이면
// 값이 옆 칸으로 밀린다. 그래서 이 5칸이 라인 위 차체의 실제 배열이다 —
// HMI 차종번호는 "지금 작업자가 고른 것"일 뿐이고, 이쪽이 "실제로 그 자리에
// 있는 것"이다. 둘을 나란히 두는 게 이 화면의 요점이다.

type Props = { stations: JigStation[]; activeJig: number | null; hmiModel: number | null };

/// 이 칸이 지금 무엇을 하고 있는가. 비트 순서가 곧 진행 순서다.
function phase(s: JigStation): { text: string; cls: string } {
  if (state(s.send_to_robot) === 'on') return { text: '로봇 전송중', cls: 'busy' };
  if (state(s.job_start) === 'on') return { text: 'JOB START', cls: 'busy' };
  if (state(s.send_done) === 'on') return { text: '전송 완료', cls: 'done' };
  if (s.work_in !== null && s.work_in !== 0) return { text: '대기', cls: 'idle' };
  if (s.work_in === null) return { text: '모름', cls: 'unknown' };
  return { text: '비어 있음', cls: 'empty' };
}

export default function JigStrip({ stations, activeJig, hmiModel }: Props) {
  if (!stations.length) return <p className="hint">JIG 수신 없음</p>;
  return (
    <div className="jig-strip">
      {stations.map(s => {
        const p = phase(s);
        const loaded = s.work_in !== null && s.work_in !== 0;
        const wid = s.work_id ?? null;
        // 실린 차체가 HMI 차종과 다르면 그 칸을 짚어준다. 어느 쪽이든 모르면
        // 표시하지 않는다 — 모름을 "정상"으로도 "위험"으로도 칠하지 않는다.
        const bad = loaded && wid !== null && wid !== 0 && hmiModel !== null && wid !== hmiModel;
        return (
          <div
            key={s.no}
            className={`jig-cell ${p.cls}${s.no === activeJig ? ' active' : ''}${bad ? ' bad' : ''}`}
          >
            <div className="jig-no">JIG {s.no}</div>
            <div className="jig-work">{loaded ? (wid ?? '?') : '—'}</div>
            <div className="jig-phase">{p.text}</div>
            <div className="jig-dist">
              {s.shift_dist ?? '—'}
              <span> / {s.job_start_dist ?? '—'}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
