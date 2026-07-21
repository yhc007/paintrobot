import { NavLink, Outlet } from 'react-router-dom';
import WeatherWidget from './components/WeatherWidget';

function NavIcon({ name }: { name: 'today' | 'live' | 'coating' | 'range' }) {
  // tiny inline icons — same visual weight as lucide
  switch (name) {
    case 'live':
      return (
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M23 7l-7 5 7 5V7z" />
          <rect x="1" y="5" width="15" height="14" rx="2" ry="2" />
        </svg>
      );
    case 'today':
      return (
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
          <line x1="16" y1="2" x2="16" y2="6" /><line x1="8" y1="2" x2="8" y2="6" /><line x1="3" y1="10" x2="21" y2="10" />
        </svg>
      );
    case 'coating':
      return (
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M12 2.69l5.66 5.66a8 8 0 1 1-11.31 0z" />
        </svg>
      );
    case 'range':
      return (
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <polyline points="3 17 9 11 13 15 21 7" />
          <polyline points="14 7 21 7 21 14" />
        </svg>
      );
  }
}

export default function App() {
  return (
    <div className="shell">
      <header className="header">
        <div className="header-top-bar" />
        <div className="header-content">
          <div className="header-left">
            <div className="brand">
              <span className="brand-bar" aria-hidden="true" />
              <div className="brand-title">
                <strong>Paintrobot</strong>
                <span className="proc">도장공정</span>
              </div>
            </div>
          </div>
          <div className="header-right">
            <div className="live-badge">
              <div className="live-dot" />
              <span>실시간 연결</span>
            </div>
            <nav className="nav-links">
              <NavLink to="/today" className="nav-btn"><NavIcon name="today" /> Today</NavLink>
              <NavLink to="/live" className="nav-btn"><NavIcon name="live" /> Live</NavLink>
              <NavLink to="/coating" className="nav-btn"><NavIcon name="coating" /> Coating</NavLink>
              <NavLink to="/range" className="nav-btn"><NavIcon name="range" /> Range</NavLink>
            </nav>
            <WeatherWidget />
          </div>
        </div>
      </header>
      <main>
        <Outlet />
      </main>
      <footer>
        <span>paint.coreon.build</span>
      </footer>
    </div>
  );
}
