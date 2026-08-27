import type { CSSProperties, ReactNode } from 'react';

// 청사진 패널 — 하드라인 테두리 + 네 모서리 레지스트레이션 마크.
// 디자인의 모든 카드가 이 골격을 공유한다.
type Props = {
  title?: ReactNode;
  right?: ReactNode;
  foot?: ReactNode;
  className?: string;
  style?: CSSProperties;
  children?: ReactNode;
};

export default function Blueprint({ title, right, foot, className, style, children }: Props) {
  return (
    <div className={`bp${className ? ` ${className}` : ''}`} style={style}>
      <i className="corner tl" aria-hidden="true" />
      <i className="corner tr" aria-hidden="true" />
      <i className="corner bl" aria-hidden="true" />
      <i className="corner br" aria-hidden="true" />
      {(title || right) && (
        <div className="bp-head">
          {title ? <div className="bp-title">{title}</div> : <span />}
          {right}
        </div>
      )}
      {children}
      {foot && <div className="bp-foot">{foot}</div>}
    </div>
  );
}
