//! Pure logic: determine match_status, derive work_date, aggregate.

use chrono::{DateTime, FixedOffset, TimeZone};
use paintrobot_schema::{DailyStats, JobIn, MatchStatus, ModelCount, PressureFactors};
use std::collections::BTreeMap;

// ── Spray-pressure recommendation ──────────────────────────────────────────
//
// Proportional control on the thickness error, multiplicative environmental
// adjustments. All gains are tunable per paint product/process; defaults are
// gentle so a single bad reading can't slam the pressure.

const KP: f64 = 0.5; // proportional gain
const T_REF: f64 = 20.0; // °C neutral
const H_REF: f64 = 50.0; // % neutral
const T_GAIN: f64 = 0.01; // 1% per °C delta
const H_GAIN: f64 = 0.003; // 0.3% per % RH delta
const P_MIN: f64 = 1.0; // bar floor
const P_MAX: f64 = 6.0; // bar ceiling
const ERR_CLAMP: f64 = 0.5; // ±50% per measurement
pub const DEFAULT_TARGET_UM: f64 = 30.0;

#[derive(Debug, Clone, Copy)]
pub struct PressureCalc {
    pub recommended: f64,
    /// (target - measured) / target, clamped. Positive ⇒ undercoated.
    pub thickness_error: f64,
    pub factors: PressureFactors,
}

pub fn recommend_pressure(
    measured_um: f64,
    target_um: f64,
    temperature_c: f64,
    humidity_pct: f64,
    current_pressure: f64,
) -> PressureCalc {
    let target = target_um.max(1.0);
    let err = ((target - measured_um) / target).clamp(-ERR_CLAMP, ERR_CLAMP);
    let control = 1.0 + KP * err;
    let temperature = 1.0 - T_GAIN * (temperature_c - T_REF);
    let humidity = 1.0 - H_GAIN * (humidity_pct - H_REF);
    let proposed = current_pressure * control * temperature * humidity;
    let recommended = proposed.clamp(P_MIN, P_MAX);
    PressureCalc {
        recommended,
        thickness_error: err,
        factors: PressureFactors { control, temperature, humidity },
    }
}

/// Determine the match status from the job's PLC/camera fields.
pub fn classify(job: &JobIn) -> MatchStatus {
    match (job.plc_model_no.as_deref(), job.camera_model_no.as_deref()) {
        (Some(a), Some(b)) if a == b => MatchStatus::Matched,
        (Some(_), Some(_)) => MatchStatus::Mismatch,
        (Some(_), None) => MatchStatus::PlcOnly,
        (None, Some(_)) => MatchStatus::CameraOnly,
        (None, None) => MatchStatus::Mismatch,
    }
}

/// Canonical model number for a job — prefer PLC, fall back to camera.
pub fn canonical_model(job: &JobIn) -> Option<&str> {
    job.plc_model_no
        .as_deref()
        .or(job.camera_model_no.as_deref())
}

/// work_date string (YYYY-MM-DD) in the given fixed offset (typically KST = +09:00).
pub fn work_date(ts: DateTime<FixedOffset>, tz: FixedOffset) -> String {
    tz.from_utc_datetime(&ts.naive_utc())
        .format("%Y-%m-%d")
        .to_string()
}

/// Aggregate rows read from CoreDB into a DailyStats bundle.
///
/// `plc_only` rows are intentionally excluded: those events are PLC state
/// updates and the dashboard's job counts should only reflect work that
/// the camera has actually verified (matched, mismatch, or camera_only).
pub fn aggregate(work_date: String, jobs: impl IntoIterator<Item = AggRow>) -> DailyStats {
    let mut per_model: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut total = 0u64;
    let mut mismatch = 0u64;
    for row in jobs {
        if row.match_status == "plc_only" {
            continue;
        }
        total += 1;
        let is_mismatch = row.match_status == "mismatch";
        if is_mismatch {
            mismatch += 1;
        }
        let entry = per_model.entry(row.model_no).or_insert((0, 0));
        entry.0 += 1;
        if is_mismatch {
            entry.1 += 1;
        }
    }
    let models = per_model
        .into_iter()
        .map(|(model_no, (job_count, mismatch_count))| ModelCount {
            model_no,
            job_count,
            mismatch_count,
        })
        .collect();
    DailyStats {
        work_date,
        total_jobs: total,
        mismatch_jobs: mismatch,
        models,
    }
}

/// Minimal row shape used by `aggregate`. Populated by `repo-coredb` after reading.
pub struct AggRow {
    pub model_no: String,
    pub match_status: String,
}

// ── PLC ↔ 카메라 지연 상관 ──────────────────────────────────────────────
//
// 엣지는 PLC 상태와 카메라 인식을 별도 이벤트로 보낸다. `classify`는 레코드
// 하나만 보므로 둘이 짝지어지지 않아 matched/mismatch가 영영 나오지 않는다.
// 여기서 사후에 이어붙인다.
//
// 카메라가 PLC보다 **앞선다**. 투입구에서 읽힌 차가 컨베이어를 타고 도장
// 부스에 도착해야 PLC 상태에 반영되기 때문이다. 그래서 ingest 시점 실시간
// 상관은 불가능하고 — 그 순간엔 짝이 될 PLC 지시가 아직 오지 않았다 —
// 하루가 지난 뒤 배치로 맞춘다.
//
// 지연은 고정이 아니다(라인 속도·재공 수에 따라 변한다). 상수로 박지 않고
// 매번 데이터에서 추정한다.

/// PLC 상태 구간의 시작점. 같은 모델이 연속으로 보고된 구간은 하나로 압축된다.
#[derive(Debug, Clone, PartialEq)]
pub struct PlcState {
    pub ts_ms: i64,
    pub model_no: String,
}

/// 카메라가 인식한 차 1대.
#[derive(Debug, Clone, PartialEq)]
pub struct CamEvent {
    pub event_id: String,
    pub ts_ms: i64,
    pub model_no: String,
    pub confidence: f64,
}

/// 한 카메라 이벤트에 대한 판정 결과.
#[derive(Debug, Clone, PartialEq)]
pub struct Reconciled {
    pub event_id: String,
    pub plc_model_no: String,
    pub status: MatchStatus,
}

/// 정합/불일치 한 쌍. 구간별로 나눠 담는다.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MatchBucket {
    pub matched: u32,
    pub mismatch: u32,
}

impl MatchBucket {
    pub fn total(&self) -> u32 {
        self.matched + self.mismatch
    }
    /// 불일치율(%). 표본이 없으면 None — 0%로 내려보내면 "이상 없음"으로 읽힌다.
    pub fn mismatch_rate(&self) -> Option<f64> {
        match self.total() {
            0 => None,
            t => Some(self.mismatch as f64 / t as f64 * 100.0),
        }
    }
    fn add(&mut self, ok: bool) {
        if ok {
            self.matched += 1;
        } else {
            self.mismatch += 1;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReconcileReport {
    /// 추정된 지연(초). 추정에 실패하면 None.
    pub offset_secs: Option<i64>,
    pub matched: u32,
    pub mismatch: u32,
    /// 신뢰도 미달 — 카메라를 못 믿으므로 판정하지 않는다.
    pub skipped_low_confidence: u32,
    /// 해당 시점에 PLC 상태가 없다 (가동 전/후).
    pub skipped_no_plc: u32,
    pub plc_states: u32,
    pub camera_events: u32,

    // ── 전환 직후 구간별 ────────────────────────────────────────────
    //
    // 구간을 **카메라 쪽 런 위치**로 나눈다. PLC 전환으로부터의 거리로 나누면
    // 우리가 추정한 오프셋의 오차가 그대로 지표에 섞인다. 런 위치는 카메라
    // 순서만으로 정해지므로 오프셋과 무관하다.
    /// 차종이 바뀐 직후 첫 대.
    pub first_unit: MatchBucket,
    /// 2~3대째.
    pub early_units: MatchBucket,
    /// 4대째 이후 — 라인이 안정된 구간.
    pub steady_units: MatchBucket,
}

/// 지연 탐색 상한(초). 이보다 긴 이송 시간은 상정하지 않는다.
pub const MAX_LAG_SECS: i64 = 900;
/// 탐색 간격(초).
pub const LAG_STEP_SECS: i64 = 30;
/// 이보다 표본이 적으면 지연을 추정하지 않는다 — 몇 대로 맞춘 오프셋은 의미가 없다.
pub const MIN_SAMPLE: usize = 10;

/// 연속된 동일 모델 PLC 이벤트를 상태 구간 하나로 압축한다.
/// 입력은 ts 오름차순이어야 한다.
pub fn plc_timeline(events: &[PlcState]) -> Vec<PlcState> {
    let mut out: Vec<PlcState> = Vec::new();
    for e in events {
        if out.last().map(|p| p.model_no != e.model_no).unwrap_or(true) {
            out.push(e.clone());
        }
    }
    out
}

/// `at_ms` 시점에 유효한 PLC 상태의 모델. 타임라인은 ts 오름차순.
fn plc_model_at(timeline: &[PlcState], at_ms: i64) -> Option<&str> {
    let idx = timeline.partition_point(|s| s.ts_ms <= at_ms);
    if idx == 0 {
        None
    } else {
        Some(timeline[idx - 1].model_no.as_str())
    }
}

/// 일치 건수가 최대가 되는 지연을 찾는다. 동률이면 짧은 쪽 — 물리적으로
/// 가장 가까운 해석을 고른다.
pub fn estimate_offset(timeline: &[PlcState], cams: &[CamEvent]) -> Option<i64> {
    if timeline.len() < 2 || cams.len() < MIN_SAMPLE {
        return None;
    }
    let mut best: Option<(i64, u32)> = None;
    let mut off = 0;
    while off <= MAX_LAG_SECS {
        let hits = cams
            .iter()
            .filter(|c| plc_model_at(timeline, c.ts_ms + off * 1000) == Some(c.model_no.as_str()))
            .count() as u32;
        if best.map(|(_, b)| hits > b).unwrap_or(true) {
            best = Some((off, hits));
        }
        off += LAG_STEP_SECS;
    }
    best.map(|(o, _)| o)
}

/// 추정한 지연으로 카메라 이벤트를 판정한다.
///
/// 신뢰도가 `min_confidence` 미만이면 판정하지 않는다. 카메라를 못 믿는
/// 상황에서 "불일치"라고 적으면 없는 품질 이상을 만들어내는 셈이다.
pub fn reconcile(
    timeline: &[PlcState],
    cams: &[CamEvent],
    min_confidence: f64,
) -> (Vec<Reconciled>, ReconcileReport) {
    let mut report = ReconcileReport {
        plc_states: timeline.len() as u32,
        camera_events: cams.len() as u32,
        ..Default::default()
    };
    let Some(offset) = estimate_offset(timeline, cams) else {
        return (Vec::new(), report);
    };
    report.offset_secs = Some(offset);

    // 런 위치는 판정 여부와 무관하게 전체 순서에서 센다 — 중간이 보류돼도
    // 뒤 차의 "몇 대째"가 밀리면 안 된다.
    let mut run_pos: Vec<u32> = Vec::with_capacity(cams.len());
    for (i, c) in cams.iter().enumerate() {
        let same = i > 0 && cams[i - 1].model_no == c.model_no;
        run_pos.push(if same { run_pos[i - 1] + 1 } else { 1 });
    }

    let mut out = Vec::new();
    for (i, c) in cams.iter().enumerate() {
        if c.confidence < min_confidence {
            report.skipped_low_confidence += 1;
            continue;
        }
        let Some(plc) = plc_model_at(timeline, c.ts_ms + offset * 1000) else {
            report.skipped_no_plc += 1;
            continue;
        };
        let ok = plc == c.model_no;
        match run_pos[i] {
            1 => report.first_unit.add(ok),
            2..=3 => report.early_units.add(ok),
            _ => report.steady_units.add(ok),
        }
        let status = if ok {
            report.matched += 1;
            MatchStatus::Matched
        } else {
            report.mismatch += 1;
            MatchStatus::Mismatch
        };
        out.push(Reconciled {
            event_id: c.event_id.clone(),
            plc_model_no: plc.to_string(),
            status,
        });
    }
    (out, report)
}

// ── 혼류 생산 지표 ──────────────────────────────────────────────────────
//
// 합계만 내면 혼류의 본질이 사라진다. 같은 100대라도 한 차종을 몰아서 만든
// 것과 여덟 차종이 섞여 흐른 것은 라인에 전혀 다른 부담이다. 순서에서만
// 나오는 값들을 여기서 뽑는다.

/// 같은 모델이 연속으로 흐른 구간.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionRun {
    pub model_no: String,
    pub count: u32,
    pub start_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixFlowStats {
    pub units: u32,
    pub models: u32,
    /// 앞 차와 모델이 달라진 횟수.
    pub changeovers: u32,
    /// 전환 횟수 / (대수-1). 1에 가까울수록 매 대마다 차종이 바뀐다.
    pub changeover_rate: f64,
    pub avg_run: f64,
    pub max_run: u32,
    /// 1대만 끼어든 투입. 혼류에서 가장 어려운 케이스 — 차 한 대 지나가는
    /// 동안 레시피를 바꿔야 한다.
    pub singles: u32,
    pub runs: Vec<ProductionRun>,
}

/// 투입 순서에서 혼류 지표를 뽑는다. 입력은 시간 오름차순.
pub fn mix_flow(seq: &[(i64, String)]) -> MixFlowStats {
    let mut runs: Vec<ProductionRun> = Vec::new();
    for (ts, model) in seq {
        match runs.last_mut() {
            Some(r) if r.model_no == *model => r.count += 1,
            _ => runs.push(ProductionRun {
                model_no: model.clone(),
                count: 1,
                start_ms: *ts,
            }),
        }
    }
    let units = seq.len() as u32;
    let changeovers = runs.len().saturating_sub(1) as u32;
    let mut distinct: Vec<&str> = runs.iter().map(|r| r.model_no.as_str()).collect();
    distinct.sort_unstable();
    distinct.dedup();
    MixFlowStats {
        units,
        models: distinct.len() as u32,
        changeovers,
        changeover_rate: if units > 1 {
            changeovers as f64 / (units - 1) as f64
        } else {
            0.0
        },
        avg_run: if runs.is_empty() {
            0.0
        } else {
            units as f64 / runs.len() as f64
        },
        max_run: runs.iter().map(|r| r.count).max().unwrap_or(0),
        singles: runs.iter().filter(|r| r.count == 1).count() as u32,
        runs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(plc: Option<&str>, cam: Option<&str>) -> JobIn {
        JobIn {
            event_id: "e".into(),
            edge_id: "edge".into(),
            plc_model_no: plc.map(String::from),
            camera_model_no: cam.map(String::from),
            plc_ts: None,
            camera_ts: None,
            confidence: None,
            image_ref: None,
        }
    }

    #[test]
    fn classify_variants() {
        assert_eq!(classify(&job(Some("A"), Some("A"))), MatchStatus::Matched);
        assert_eq!(classify(&job(Some("A"), Some("B"))), MatchStatus::Mismatch);
        assert_eq!(classify(&job(Some("A"), None)), MatchStatus::PlcOnly);
        assert_eq!(classify(&job(None, Some("A"))), MatchStatus::CameraOnly);
    }

    #[test]
    fn pressure_at_target_keeps_pressure() {
        let p = recommend_pressure(30.0, 30.0, 20.0, 50.0, 3.5);
        // factors all 1.0 ⇒ recommended == current
        assert!((p.recommended - 3.5).abs() < 1e-6);
        assert!(p.thickness_error.abs() < 1e-6);
    }

    #[test]
    fn pressure_under_target_increases() {
        let p = recommend_pressure(24.0, 30.0, 20.0, 50.0, 3.5);
        assert!(p.recommended > 3.5);
    }

    #[test]
    fn pressure_over_target_decreases() {
        let p = recommend_pressure(36.0, 30.0, 20.0, 50.0, 3.5);
        assert!(p.recommended < 3.5);
    }

    #[test]
    fn high_temp_reduces_pressure() {
        // measured=target so control factor is 1; only temp moves it
        let p = recommend_pressure(30.0, 30.0, 30.0, 50.0, 3.5);
        assert!(p.recommended < 3.5);
        assert!(p.factors.temperature < 1.0);
    }

    #[test]
    fn high_humidity_reduces_pressure() {
        let p = recommend_pressure(30.0, 30.0, 20.0, 80.0, 3.5);
        assert!(p.recommended < 3.5);
        assert!(p.factors.humidity < 1.0);
    }

    #[test]
    fn pressure_clamped_to_bounds() {
        // catastrophically thin film + already high pressure shouldn't exceed P_MAX
        let p = recommend_pressure(1.0, 100.0, 0.0, 0.0, 6.0);
        assert!(p.recommended <= P_MAX);
        // catastrophically thick film + already low pressure shouldn't go under P_MIN
        let p = recommend_pressure(100.0, 1.0, 40.0, 100.0, 1.0);
        assert!(p.recommended >= P_MIN);
    }

    #[test]
    fn aggregate_counts() {
        let rows = vec![
            AggRow { model_no: "A".into(), match_status: "matched".into() },
            AggRow { model_no: "A".into(), match_status: "mismatch".into() },
            AggRow { model_no: "B".into(), match_status: "matched".into() },
        ];
        let stats = aggregate("2026-04-23".into(), rows);
        assert_eq!(stats.total_jobs, 3);
        assert_eq!(stats.mismatch_jobs, 1);
        assert_eq!(stats.models.len(), 2);
    }

    #[test]
    fn aggregate_excludes_plc_only_rows() {
        let rows = vec![
            AggRow { model_no: "A".into(), match_status: "plc_only".into() },
            AggRow { model_no: "A".into(), match_status: "plc_only".into() },
            AggRow { model_no: "A".into(), match_status: "camera_only".into() },
            AggRow { model_no: "B".into(), match_status: "matched".into() },
            AggRow { model_no: "C".into(), match_status: "mismatch".into() },
        ];
        let stats = aggregate("2026-04-28".into(), rows);
        // 2 PLC-only rows excluded; 3 camera-confirmed rows counted
        assert_eq!(stats.total_jobs, 3);
        assert_eq!(stats.mismatch_jobs, 1);
        // Model A only counts the camera_only row, PLC-only rows skipped
        let a = stats.models.iter().find(|m| m.model_no == "A").unwrap();
        assert_eq!(a.job_count, 1);
        assert_eq!(a.mismatch_count, 0);
    }

    #[test]
    fn run_position_buckets_split_changeover_from_steady() {
        // PLC는 계속 "1". 카메라는 전환 직후 첫 대만 다른 차종을 본다.
        let timeline = vec![plc(0, "1"), plc(1000, "9")];
        let mut cams = Vec::new();
        // [2] 한 대 → 첫 대 불일치
        cams.push(cam("c0", 0, "2", 0.95));
        // [1] 열 대 → 첫 대는 정합, 나머지는 2~3대째/안정 구간
        for i in 1..11 {
            cams.push(cam(&format!("c{i}"), i, "1", 0.95));
        }
        let (_, rep) = reconcile(&timeline, &cams, 0.7);
        // 런: [2 x1][1 x10] → 첫 대는 2건(c0, c1)
        assert_eq!(rep.first_unit.total(), 2);
        assert_eq!(rep.first_unit.mismatch, 1);
        assert_eq!(rep.first_unit.mismatch_rate(), Some(50.0));
        // 2~3대째는 2건, 전부 정합
        assert_eq!(rep.early_units.total(), 2);
        assert_eq!(rep.early_units.mismatch, 0);
        // 나머지는 안정 구간
        assert_eq!(rep.steady_units.total(), 7);
        assert_eq!(rep.first_unit.total() + rep.early_units.total() + rep.steady_units.total(),
                   rep.matched + rep.mismatch);
    }

    #[test]
    fn empty_bucket_has_no_rate_rather_than_zero() {
        // 표본이 없을 때 0%를 돌려주면 "이상 없음"으로 읽힌다
        let b = MatchBucket::default();
        assert_eq!(b.mismatch_rate(), None);
        assert_eq!(b.total(), 0);
    }

    #[test]
    fn skipped_reads_do_not_shift_run_position() {
        let timeline = vec![plc(0, "1"), plc(1000, "9")];
        let mut cams = vec![cam("blurry", 0, "1", 0.10)];
        for i in 1..12 {
            cams.push(cam(&format!("c{i}"), i, "1", 0.95));
        }
        let (_, rep) = reconcile(&timeline, &cams, 0.7);
        // 첫 대(blurry)는 보류됐지만 2대째는 여전히 2대째다 — 첫 대 버킷은 비고,
        // 2~3대째에 2건이 들어간다.
        assert_eq!(rep.skipped_low_confidence, 1);
        assert_eq!(rep.first_unit.total(), 0);
        assert_eq!(rep.early_units.total(), 2);
    }

    // ── 혼류 지표 ──────────────────────────────────────────────────────
    fn seq(models: &[&str]) -> Vec<(i64, String)> {
        models.iter().enumerate().map(|(i, m)| (i as i64 * 60_000, m.to_string())).collect()
    }

    #[test]
    fn batch_run_is_not_mixed_flow() {
        let s = mix_flow(&seq(&["1"; 10]));
        assert_eq!(s.units, 10);
        assert_eq!(s.changeovers, 0);
        assert_eq!(s.avg_run, 10.0);
        assert_eq!(s.singles, 0);
        assert_eq!(s.changeover_rate, 0.0);
    }

    #[test]
    fn alternating_every_unit_is_maximally_mixed() {
        let s = mix_flow(&seq(&["1", "2", "1", "2", "1"]));
        assert_eq!(s.changeovers, 4);
        assert!((s.changeover_rate - 1.0).abs() < 1e-9);
        assert_eq!(s.singles, 5);
        assert_eq!(s.max_run, 1);
    }

    /// 실제 라인에서 본 모양 — 긴 구간 사이에 1대가 끼어든다.
    #[test]
    fn counts_the_single_unit_insertions() {
        let s = mix_flow(&seq(&["1", "1", "1", "2", "1", "1", "5", "5", "5", "6"]));
        assert_eq!(s.units, 10);
        assert_eq!(s.models, 4);
        // 런: [1x3][2x1][1x2][5x3][6x1]
        assert_eq!(s.changeovers, 4);
        assert_eq!(s.singles, 2); // "2" 한 대, "6" 한 대
        assert_eq!(s.max_run, 3);
        assert_eq!(s.runs.len(), 5);
        assert_eq!(s.runs[1].model_no, "2");
        assert_eq!(s.runs[1].count, 1);
    }

    #[test]
    fn empty_day_does_not_divide_by_zero() {
        let s = mix_flow(&[]);
        assert_eq!(s.units, 0);
        assert_eq!(s.avg_run, 0.0);
        assert_eq!(s.changeover_rate, 0.0);
        assert!(s.runs.is_empty());
    }

    // ── 지연 상관 ──────────────────────────────────────────────────────
    fn plc(ts_min: i64, m: &str) -> PlcState {
        PlcState { ts_ms: ts_min * 60_000, model_no: m.into() }
    }
    fn cam(id: &str, ts_min: i64, m: &str, conf: f64) -> CamEvent {
        CamEvent { event_id: id.into(), ts_ms: ts_min * 60_000, model_no: m.into(), confidence: conf }
    }

    #[test]
    fn timeline_compresses_repeats() {
        let raw = vec![plc(0, "6"), plc(1, "6"), plc(2, "6"), plc(9, "8"), plc(10, "8"), plc(20, "6")];
        let t = plc_timeline(&raw);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].model_no, "6");
        assert_eq!(t[1].ts_ms, 9 * 60_000);
        assert_eq!(t[2].model_no, "6");
    }

    /// 카메라가 PLC보다 6분 앞서는 라인. 오프셋을 그만큼 되찾아야 한다.
    #[test]
    fn estimates_the_lag_the_line_actually_has() {
        let timeline = vec![plc(0, "1"), plc(60, "3"), plc(120, "4")];
        // 카메라는 각 전환보다 6분 먼저 그 모델을 본다
        let cams: Vec<_> = (0..6).map(|i| cam(&format!("a{i}"), 10 + i * 5, "1", 0.95))
            .chain((0..6).map(|i| cam(&format!("b{i}"), 54 + i * 5, if i == 0 { "3" } else { "3" }, 0.95)))
            .collect();
        let off = estimate_offset(&timeline, &cams).unwrap();
        assert!((300..=420).contains(&off), "offset {off} 이 6분 근방이 아님");
    }

    #[test]
    fn refuses_to_guess_from_a_tiny_sample() {
        let timeline = vec![plc(0, "1"), plc(60, "3")];
        let cams = vec![cam("a", 5, "1", 0.95), cam("b", 6, "1", 0.95)];
        assert_eq!(estimate_offset(&timeline, &cams), None);
        let (out, rep) = reconcile(&timeline, &cams, 0.7);
        assert!(out.is_empty());
        assert_eq!(rep.offset_secs, None);
        assert_eq!(rep.matched + rep.mismatch, 0);
    }

    #[test]
    fn low_confidence_reads_are_not_judged() {
        let timeline = vec![plc(0, "1"), plc(60, "3")];
        let mut cams: Vec<_> = (0..12).map(|i| cam(&format!("ok{i}"), 1 + i, "1", 0.95)).collect();
        cams.push(cam("blurry", 20, "9", 0.40));
        let (out, rep) = reconcile(&timeline, &cams, 0.7);
        assert_eq!(rep.skipped_low_confidence, 1);
        // 흐린 판독은 결과에 아예 등장하지 않는다 — 없는 불일치를 만들지 않는다
        assert!(out.iter().all(|r| r.event_id != "blurry"));
    }

    #[test]
    fn genuine_wrong_model_is_flagged() {
        let timeline = vec![plc(0, "1"), plc(600, "3")];
        let mut cams: Vec<_> = (0..12).map(|i| cam(&format!("ok{i}"), 1 + i, "1", 0.95)).collect();
        cams.push(cam("stray", 20, "2", 0.94));
        let (out, rep) = reconcile(&timeline, &cams, 0.7);
        assert_eq!(rep.mismatch, 1);
        let stray = out.iter().find(|r| r.event_id == "stray").unwrap();
        assert_eq!(stray.status, MatchStatus::Mismatch);
        assert_eq!(stray.plc_model_no, "1");
    }

    #[test]
    fn events_before_any_plc_state_are_skipped() {
        let timeline = vec![plc(100, "1")];
        let cams: Vec<_> = (0..12).map(|i| cam(&format!("e{i}"), i, "1", 0.95)).collect();
        // 타임라인이 1구간뿐이라 추정 자체를 거부한다
        assert_eq!(estimate_offset(&timeline, &cams), None);

        let timeline = vec![plc(100, "1"), plc(200, "3")];
        let (_, rep) = reconcile(&timeline, &cams, 0.7);
        assert_eq!(rep.skipped_no_plc + rep.matched + rep.mismatch, 12);
    }
}
