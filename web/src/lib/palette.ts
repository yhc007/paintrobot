// 모델별 계열 색상.
//
// 어두운 네이비 패널(#1d2d3d) 위에서 검증한 순서다. 인접 쌍 기준으로
// 색약 분리 ΔE 8.4, 정상시야 ΔE 17.5, 전 슬롯 대비 3:1 이상을 만족한다.
// 순서 자체가 안전장치이므로 임의로 뒤섞지 말 것.
//
// 빨강 계열은 일부러 뺐다. 참조 팔레트의 빨강(#e66767)은 불일치 표시색
// (--pb-bad #d9614c)과 ΔE 4.1 — 사실상 같은 색이라 모델 하나가 불일치
// 막대로 보인다. 그 자리를 청록(#0f9fa8)이 대신한다.
export const SERIES_COLORS = [
  '#3987e5', // 파랑
  '#d95926', // 주황
  '#199e70', // 아쿠아
  '#c98500', // 노랑
  '#d55181', // 마젠타
  '#1aa81a', // 초록
  '#9085e9', // 보라
  '#0f9fa8', // 청록
] as const;

// 9번째부터는 새 색을 만들지 않고 여기로 접는다 (--pb-steel).
export const OTHER_COLOR = '#5980a6';
export const OTHER_LABEL = '기타';

export type ModelPalette = {
  /** 고정 색이 배정된 모델 → 색상. 스택 순서도 이 Map의 삽입 순서를 따른다. */
  colors: Map<string, string>;
  /** `기타`로 접힌 모델 목록 (툴팁에서 펼쳐 보여준다). */
  folded: string[];
};

/**
 * 모델번호 → 슬롯 번호. **화면에 어떤 모델이 떠 있는지와 무관하게** 모델번호
 * 하나만 보고 정한다.
 *
 * 이게 중요한 이유: 현황 페이지에는 오늘 생산한 모델만, 실적 페이지에는 조회
 * 구간의 모델이 뜬다. 화면에 있는 집합을 보고 색을 나눠주면 같은 3번 모델이
 * 현황에서는 주황, 실적에서는 아쿠아로 나온다. 두 화면을 나란히 놓고 보는
 * 사람에게는 그냥 틀린 그래프다.
 *
 * 모델번호가 숫자면 그 값을 그대로 쓰고 (1번 → 1번 슬롯), 아니면 문자열
 * 해시로 고정 배정한다.
 */
function slotFor(model_no: string): number {
  const n = Number(model_no);
  if (Number.isInteger(n) && n >= 1) return (n - 1) % SERIES_COLORS.length;
  let h = 0;
  for (let i = 0; i < model_no.length; i += 1) {
    h = (h * 31 + model_no.charCodeAt(i)) >>> 0;
  }
  return h % SERIES_COLORS.length;
}

/**
 * 모델 → 색 배정. 두 가지를 따로 정한다.
 *
 * - **어떤 모델이 고유색을 받는가**는 생산량 상위 8종. 물량이 많은 모델이
 *   그래프에서 식별 가능해야 하기 때문.
 * - **어떤 색을 받는가**는 `slotFor` — 모델번호에서 유도하므로 화면·기간이
 *   바뀌어도 같은 모델은 같은 색이다.
 *
 * 슬롯이 겹치면(모델 9종 이상에서만 생길 수 있다) 모델번호 순으로 훑으며
 * 먼저 온 쪽이 갖고 나중 쪽은 다음 빈 슬롯으로 밀린다 — 순서를 고정해야
 * 결과가 매번 같다.
 */
export function buildModelPalette(models: Iterable<{ model_no: string; job_count: number }>): ModelPalette {
  const totals = new Map<string, number>();
  for (const m of models) {
    totals.set(m.model_no, (totals.get(m.model_no) ?? 0) + m.job_count);
  }

  const ranked = Array.from(totals, ([model_no, total]) => ({ model_no, total }))
    .sort((a, b) => b.total - a.total || a.model_no.localeCompare(b.model_no, 'ko'));

  const named = ranked.slice(0, SERIES_COLORS.length)
    .map(r => r.model_no)
    .sort((a, b) => a.localeCompare(b, 'ko', { numeric: true }));

  const taken = new Set<number>();
  const colors = new Map<string, string>();
  for (const model_no of named) {
    let slot = slotFor(model_no);
    while (taken.has(slot)) slot = (slot + 1) % SERIES_COLORS.length;
    taken.add(slot);
    colors.set(model_no, SERIES_COLORS[slot]);
  }

  return {
    colors,
    folded: ranked.slice(SERIES_COLORS.length)
      .map(r => r.model_no)
      .sort((a, b) => a.localeCompare(b, 'ko', { numeric: true })),
  };
}

export function colorFor(palette: ModelPalette, model_no: string): string {
  return palette.colors.get(model_no) ?? OTHER_COLOR;
}
