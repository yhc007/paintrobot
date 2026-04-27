import { useQuery } from '@tanstack/react-query';
import { api } from '../lib/api';

export default function WeatherWidget() {
  const q = useQuery({
    queryKey: ['weather'],
    queryFn: api.weather,
    refetchInterval: 60_000,
  });

  if (q.isLoading) return <div className="weather">…</div>;
  if (q.error || !q.data) return <div className="weather">--°C / --%</div>;
  const w = q.data;
  return (
    <div className="weather" title={`${w.source} · ${w.observed_at}`}>
      <span className="t">{w.temperature_c.toFixed(1)}°C</span>
      <span className="h">{Math.round(w.humidity_pct)}%</span>
    </div>
  );
}
