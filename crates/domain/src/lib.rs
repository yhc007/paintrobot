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
}
