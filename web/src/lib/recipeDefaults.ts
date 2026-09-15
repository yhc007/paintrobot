import type { PlcRecipe, RecipeAxis } from './api';

// 화면에서 쓰는 기본 레시피.
//
// PLC에서 아직 수신되지 않은 차종의 자리를 채우기 위한 값이며, 실제 설비값이
// 아니다. DB에는 넣지 않는다 — 사용자가 화면에서 저장을 눌렀을 때만 서버로
// 올라간다. 그래야 실제 수신분과 섞이지 않는다.
//
// 차종명은 지어내지 않는다. 실제로 받은 것은 모델 1='77', 8='140' 두 개뿐이고
// 나머지는 이름을 모르므로 비워 둔다.
export const MODEL_NOS = [1, 2, 3, 4, 5, 6, 7, 8] as const;

export const DEFAULT_LEVELS = 8;

/** 단계 수에 맞춰 길이를 맞춘다. 짧으면 0으로 채우고, 길면 자른다. */
export function fitAxis(a: RecipeAxis | undefined, levels: number): RecipeAxis {
  const fit = (xs: number[] = []) =>
    Array.from({ length: levels }, (_, i) => xs[i] ?? 0);
  return { table: fit(a?.table), applied: fit(a?.applied) };
}

/** 앞 n단만 값이 있고 나머지는 0인 배열. 실제 수신 데이터가 이런 모양이다. */
function ramp(value: number, used: number, levels = DEFAULT_LEVELS): number[] {
  return Array.from({ length: levels }, (_, i) => (i < used ? value : 0));
}

/// 기본값. 실제 수신 데이터(모델 1·8)의 형태를 참고해 구성했다.
export function defaultRecipe(modelNo: number): PlcRecipe {
  const used = 4;
  return {
    model_no: modelNo,
    model_name: null,
    levels: DEFAULT_LEVELS,
    edge_id: null,
    received_at: null,
    work_date: null,
    recipe: {
      atomization: { table: ramp(50, used + 1), applied: ramp(42, used) },
      pattern: { table: ramp(30, used + 1), applied: ramp(20, used) },
      flow: { table: ramp(0, used + 1), applied: ramp(0, used) },
    },
  };
}
