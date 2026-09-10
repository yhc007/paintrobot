import { useEffect, useRef, useState } from 'react';

// 같은 사이트에서 돌아가는 다른 관제 화면들. 주소는 실제 응답을 확인해
// 넣었다 — 각 항목의 note는 그 서비스가 스스로 붙인 제목이다.
const APPS = [
  { label: 'R-S LINE HMI', note: '현대정밀 RSpring 실시간 모니터링', href: 'https://rspring.coreon.build' },
  { label: 'AAS Browser', note: 'Asset Administration Shell', href: 'https://aas.coreon.build' },
  { label: 'HDM Monitoring', note: '도메인 프로브 상태', href: 'https://hdm-m.coreon.build' },
  { label: 'umati', note: 'FOCAS → OPC UA 라인 모니터', href: 'https://umati.coreon.build' },
  { label: 'hdm-3d', note: '공장 배치 디자인', href: 'https://hdm-3d.coreon.build' },
];

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
