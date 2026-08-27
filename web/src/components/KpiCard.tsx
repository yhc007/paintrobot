import Blueprint from './Blueprint';

type Props = {
  label: string;
  value: string | number;
  unit?: string;
  sub?: string;
  accent?: 'normal' | 'ice' | 'ok' | 'warn' | 'bad' | 'idle';
};

// 계기 타일 — 라벨 / 대형 수치 / 보조 설명 한 줄.
export default function KpiCard({ label, value, unit, sub, accent = 'normal' }: Props) {
  return (
    <Blueprint className="tile">
      <div className="gauge-label">{label}</div>
      <div className={`gauge-value${accent === 'normal' ? '' : ` ${accent}`}`}>
        {value}
        {unit && <span className="gauge-unit"> {unit}</span>}
      </div>
      {sub && <div className="tile-sub">{sub}</div>}
    </Blueprint>
  );
}
