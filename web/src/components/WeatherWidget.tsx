import { useQuery } from '@tanstack/react-query';
import { api } from '../lib/api';

const DOW = ['일', '월', '화', '수', '목', '금', '토'];

function todayLabel(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} (${DOW[d.getDay()]})`;
}

// 헤더 환경 스트립 — 부스 온·습도 + 날짜.
export default function WeatherWidget() {
  const q = useQuery({
    queryKey: ['weather'],
    queryFn: api.weather,
    refetchInterval: 60_000,
  });
  const w = q.data;
  return (
    <div className="env-strip" title={w ? `${w.source} · ${w.observed_at}` : undefined}>
      <span>부스 {w ? `${w.temperature_c.toFixed(1)}℃` : '--℃'}</span>
      <span>습도 {w ? `${Math.round(w.humidity_pct)}%` : '--%'}</span>
      <span>{todayLabel()}</span>
    </div>
  );
}
