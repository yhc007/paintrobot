type Props = { label: string; value: string | number; accent?: 'normal' | 'warn' };

export default function KpiCard({ label, value, accent = 'normal' }: Props) {
  return (
    <div className={`kpi kpi-${accent}`}>
      <div className="kpi-label">{label}</div>
      <div className="kpi-value">{value}</div>
    </div>
  );
}
