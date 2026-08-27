import { NavLink, Outlet } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { api } from './lib/api';
import { matchState } from './components/PlcCard';
import WeatherWidget from './components/WeatherWidget';
import Clock from './components/Clock';

const TABS = [
  { to: '/today', label: '현황' },
  { to: '/live', label: '라인·정합' },
  { to: '/coating', label: '도막·레시피' },
  { to: '/range', label: '실적' },
];

export default function App() {
  // 헤더의 실시간 배지는 PLC 신호 유무로 판정한다 (Today와 캐시 공유).
  const plc = useQuery({
    queryKey: ['plc', 'current'],
    queryFn: api.plcCurrent,
    refetchInterval: 30_000,
  });
  const linked = matchState(plc.data) !== 'none';

  return (
    <div className="shell">
      <header className="header">
        <div className="brand">
          <span className="brand-bar" aria-hidden="true" />
          <div className="brand-name">PAINTROBOT</div>
          <div className="brand-sub">도장공정 관제</div>
        </div>

        <nav className="nav-links">
          {TABS.map(t => (
            <NavLink key={t.to} to={t.to} className="nav-btn">{t.label}</NavLink>
          ))}
        </nav>

        <div className="header-right">
          <div className={`live-badge${linked ? '' : ' off'}`}>
            <span className="live-dot" />
            <span>{linked ? 'LINE 01 · 실시간' : 'LINE 01 · 신호 없음'}</span>
          </div>
          <WeatherWidget />
          <Clock />
        </div>
      </header>

      <main>
        <Outlet />
      </main>

      <footer>
        <span>paint.coreon.build</span>
        <span>현대정밀 R1 도장라인</span>
      </footer>
    </div>
  );
}
