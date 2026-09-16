import type { Bit, BitMap } from '../lib/api';
import { bitTone, label } from '../lib/robotLabels';

// 비트는 **세 가지 상태**로 그린다. on / off / 모름.
//
// `null`을 off와 같이 그리면 안 된다. `null`은 "그 워드 블록을 못 읽었다 =
// 값을 모른다"는 뜻이고, 비상정지 비트가 그렇게 접히면 화면이 "안전함"으로
// 읽힌다. 스펙(`plc_r.md` §2)이 세 상태를 구분해 저장하라고 못박은 이유다.
export function state(bit: Bit): 'on' | 'off' | 'unknown' {
  if (bit === true) return 'on';
  if (bit === false) return 'off';
  return 'unknown';
}

export function BitChip({ group, name, bit }: { group: string; name: string; bit: Bit }) {
  const st = state(bit);
  const tone = bitTone(group, name);
  // 극성을 아는 것만 빨강으로 칠한다. P 영역 접점은 ON이 이상인지 정상인지
  // 아직 모르므로 물음표를 달아 두고 색으로 단정하지 않는다.
  const cls = st === 'on' && tone !== 'plain' ? ` ${tone}` : '';
  const text = st === 'on' ? 'ON' : st === 'off' ? 'OFF' : '모름';
  const hint =
    tone === 'unconfirmed' ? ' · 접점 극성 미확인(NC 가능)' : '';
  return (
    <div className={`bit ${st}${cls}`} title={`${group}.${name} = ${text}${hint}`}>
      <i className="bit-dot" aria-hidden="true" />
      <span className="bit-name">{label(name)}</span>
      <span className="bit-val">
        {text}
        {st === 'on' && tone === 'unconfirmed' && <sup aria-label="극성 미확인">?</sup>}
      </span>
    </div>
  );
}

/// 비트 묶음 하나. 키 순서는 서버가 준 순서를 그대로 쓴다.
export function BitGroup({
  title,
  group,
  bits,
}: {
  title: string;
  group: string;
  bits: BitMap | undefined;
}) {
  const entries = Object.entries(bits ?? {});
  return (
    <div className="bit-group">
      <div className="bit-group-head">
        {title}
        <span className="bit-group-count">{entries.length}</span>
      </div>
      {entries.length === 0 ? (
        <p className="hint">수신 없음</p>
      ) : (
        <div className="bit-grid">
          {entries.map(([k, v]) => (
            <BitChip key={k} group={group} name={k} bit={v} />
          ))}
        </div>
      )}
    </div>
  );
}

/// 세 상태가 무엇인지 화면에 한 번 적어 둔다. 안전 관련 구분이라
/// 읽는 사람이 추측하게 두지 않는다.
export function BitLegend() {
  return (
    <div className="bit-legend">
      <span className="bit on"><i className="bit-dot" />ON</span>
      <span className="bit off"><i className="bit-dot" />OFF</span>
      <span className="bit unknown"><i className="bit-dot" />모름 · 읽기 실패</span>
      <span className="bit on unconfirmed"><i className="bit-dot" />ON · 극성 미확인</span>
    </div>
  );
}
