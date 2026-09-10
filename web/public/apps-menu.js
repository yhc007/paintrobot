/* 자동 생성 파일 — 직접 고치지 말 것. 원본: web/apps.json
 * 다시 만들려면: npm run build (또는 node scripts/gen-apps-menu.mjs)
 *
 * 쓰는 법: 각 서비스 HTML에 아래 한 줄을 넣는다.
 *   <script src="https://paint.coreon.build/apps-menu.js" defer></script>
 *
 * 스타일은 Shadow DOM 안에 있어 심는 쪽 CSS와 서로 영향을 주지 않는다.
 */
(function () {
  'use strict';
  if (window.__coreonAppsMenu) return;   // 두 번 실려도 하나만
  window.__coreonAppsMenu = true;

  var APPS = [
    {
      "label": "R-S LINE HMI",
      "note": "현대정밀 RSpring 실시간 모니터링",
      "href": "https://dgx-rspring.coreon.build"
    },
    {
      "label": "AAS Browser",
      "note": "Asset Administration Shell",
      "href": "https://aas.coreon.build"
    },
    {
      "label": "HDM Monitoring",
      "note": "도메인 프로브 상태",
      "href": "https://hdm-m.coreon.build"
    },
    {
      "label": "umati",
      "note": "FOCAS → OPC UA 라인 모니터",
      "href": "https://umati.coreon.build"
    },
    {
      "label": "hdm-3d",
      "note": "공장 배치 디자인",
      "href": "https://hdm-3d.coreon.build"
    },
    {
      "label": "PAINTROBOT",
      "note": "도장공정 관제",
      "href": "https://paint.coreon.build"
    }
  ];

  // 지금 보고 있는 화면은 메뉴에서 뺀다 — 자기 자신으로 가는 링크는 혼란만 준다.
  var here = location.origin;
  var items = APPS.filter(function (a) { return a.href.replace(/\/$/, '') !== here; });

  var host = document.createElement('div');
  host.id = 'coreon-apps-menu';
  var root = host.attachShadow({ mode: 'open' });

  root.innerHTML = [
    '<style>',
    ':host{position:fixed;top:10px;left:10px;z-index:2147483000;',
    '  font-family:system-ui,-apple-system,"Segoe UI",sans-serif}',
    '.btn{display:flex;align-items:center;justify-content:center;width:38px;height:38px;',
    '  padding:0;cursor:pointer;background:#16232f;border:1px solid rgba(230,237,244,.28);border-radius:0}',
    '.btn:hover,.btn.on{background:#22384c;border-color:#94bce3}',
    '.bars{display:flex;flex-direction:column;gap:4px;width:18px}',
    '.bars i{height:2px;background:#cfe0ef}',
    '.btn.on .bars i{background:#94bce3}',
    '.menu{position:absolute;top:46px;left:0;min-width:250px;background:#16232f;',
    '  border:1px solid rgba(230,237,244,.28);box-shadow:0 10px 28px rgba(0,0,0,.5)}',
    '.head{padding:10px 14px 8px;font-size:13px;letter-spacing:.08em;color:#7e9cb8;',
    '  border-bottom:1px solid rgba(230,237,244,.14)}',
    'a{display:block;padding:10px 14px;text-decoration:none;color:#e6edf4;',
    '  border-bottom:1px solid rgba(230,237,244,.14)}',
    'a:last-child{border-bottom:0}',
    'a:hover{background:#22384c}',
    '.l{display:block;font-size:15px}',
    '.n{display:block;margin-top:2px;font-size:12px;color:#7e9cb8}',
    '</style>',
    '<button class="btn" type="button" aria-label="다른 관제 화면 열기" aria-expanded="false">',
    '  <span class="bars"><i></i><i></i><i></i></span>',
    '</button>',
    '<div class="menu" hidden><div class="head">관제 화면</div>',
    items.map(function (a) {
      return '<a href="' + a.href + '" target="_blank" rel="noreferrer">' +
             '<span class="l"></span><span class="n"></span></a>';
    }).join(''),
    '</div>',
  ].join('');

  // 라벨은 textContent로 넣는다 — apps.json에 꺾쇠가 들어가도 안전하다.
  var links = root.querySelectorAll('a');
  items.forEach(function (a, i) {
    links[i].querySelector('.l').textContent = a.label;
    links[i].querySelector('.n').textContent = a.note;
  });

  var btn = root.querySelector('.btn');
  var menu = root.querySelector('.menu');
  var open = false;
  function set(v) {
    open = v;
    menu.hidden = !v;
    btn.classList.toggle('on', v);
    btn.setAttribute('aria-expanded', String(v));
  }
  btn.addEventListener('click', function (e) { e.stopPropagation(); set(!open); });
  root.addEventListener('click', function (e) { e.stopPropagation(); });
  document.addEventListener('click', function () { if (open) set(false); });
  document.addEventListener('keydown', function (e) { if (e.key === 'Escape' && open) set(false); });

  (document.body || document.documentElement).appendChild(host);
})();
