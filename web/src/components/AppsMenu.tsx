import { useEffect, useRef, useState } from 'react';

// 링크 목록의 단일 출처는 `web/apps.json`이다. 같은 파일에서 다른 서비스에
// 심는 `public/apps-menu.js`도 생성되므로, 주소를 바꿀 곳은 그 한 곳뿐이다.
import APPS_ALL from '../../apps.json';

// 자기 자신은 메뉴에서 뺀다.
const APPS = (APPS_ALL as { label: string; note: string; href: string }[]).filter(
  a => !a.href.includes('paint.coreon.build'),
);

export default function AppsMenu() {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  // 바깥 클릭과 Esc로 닫는다. 헤더에 붙어 있어 열린 채로 두면 화면을 가린다.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div className="apps" ref={wrapRef}>
      <button
        type="button"
        className={`apps-btn${open ? ' on' : ''}`}
        aria-label="다른 관제 화면 열기"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen(v => !v)}
      >
        <span className="apps-bars" aria-hidden="true">
          <i /><i /><i />
        </span>
      </button>

      {open && (
        <div className="apps-menu" role="menu">
          <div className="apps-menu-head">관제 화면</div>
          {APPS.map(a => (
            <a
              key={a.href}
              className="apps-item"
              role="menuitem"
              href={a.href}
              target="_blank"
              rel="noreferrer"
              onClick={() => setOpen(false)}
            >
              <span className="apps-item-label">{a.label}</span>
              <span className="apps-item-note">{a.note}</span>
            </a>
          ))}
        </div>
      )}
    </div>
  );
}
