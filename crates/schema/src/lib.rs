//! DTOs shared between edge clients, API gateway, and the frontend.

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Matched,
    Mismatch,
    PlcOnly,
    CameraOnly,
}

impl MatchStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchStatus::Matched => "matched",
            MatchStatus::Mismatch => "mismatch",
            MatchStatus::PlcOnly => "plc_only",
            MatchStatus::CameraOnly => "camera_only",
        }
    }
}

/// Incoming payload for `POST /api/v1/jobs`. The edge determines the match itself
/// and sends one record per completed reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobIn {
    pub event_id: String,
    pub edge_id: String,
    pub plc_model_no: Option<String>,
    pub camera_model_no: Option<String>,
    pub plc_ts: Option<DateTime<FixedOffset>>,
    pub camera_ts: Option<DateTime<FixedOffset>>,
    pub confidence: Option<f64>,
    pub image_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    pub accepted: u32,
    pub duplicates: u32,
    pub rejected: Vec<Rejected>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rejected {
    pub event_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchIn {
    pub edge_id: String,
    pub jobs: Vec<JobIn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCount {
    pub model_no: String,
    pub job_count: u64,
    pub mismatch_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub work_date: String, // YYYY-MM-DD
    pub total_jobs: u64,
    pub mismatch_jobs: u64,
    pub models: Vec<ModelCount>,
}

/// PLC-only state update. The PLC reports the model it is currently working on.
/// The server records it as a `plc_only` job entry — emit one per state change
/// (do not poll into a duplicate stream).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlcModelIn {
    pub edge_id: String,
    pub model_no: String,
    /// PLC reading timestamp. If omitted the server uses receive time.
    pub plc_ts: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlcCurrent {
    pub model_no: Option<String>,
    pub edge_id: Option<String>,
    pub plc_ts: Option<i64>,
    pub event_id: Option<String>,
    /// Most recent camera-side recognition (independent of the PLC event).
    pub camera_model_no: Option<String>,
    pub camera_ts: Option<i64>,
}

/// Edge-supplied coating-thickness measurement. The server computes the
/// recommended spray pressure from this plus current temperature/humidity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoatingIn {
    pub event_id: String,
    pub model_no: String,
    pub measured_um: f64,
    /// If omitted, server falls back to a per-model default (or 30μm).
    pub target_um: Option<f64>,
    pub current_pressure: f64,
    /// If omitted, server fetches the latest OWM reading.
    pub temperature_c: Option<f64>,
    pub humidity_pct: Option<f64>,
    pub job_event_id: Option<String>,
    pub edge_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoatingOut {
    pub event_id: String,
    pub model_no: String,
    pub measured_um: f64,
    pub target_um: f64,
    pub current_pressure: f64,
    pub recommended_pressure: f64,
    pub thickness_error: f64,
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub factors: PressureFactors,
    pub measured_at: i64,
    pub work_date: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PressureFactors {
    pub control: f64,
    pub temperature: f64,
    pub humidity: f64,
}

/// One paint parameter's master (`table`) vs currently-applied (`applied`)
/// values. Each vector holds `levels` entries; values are INT16-range integers
/// (0 is valid — e.g. the spray gun is idle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeParam {
    pub table: Vec<i64>,
    pub applied: Vec<i64>,
}

/// The three paint parameters the edge reader pulls from the PLC per car model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeSet {
    pub atomization: RecipeParam, // 무화
    pub pattern: RecipeParam,     // 패턴
    pub flow: RecipeParam,        // 토출량
}

/// Incoming payload for `POST /api/v1/plc/recipe`. The edge reader posts the
/// full paint recipe for the car model currently selected on the PLC/HMI.
/// `model_no` is an integer (HMI selection 1~8) — note this differs from the
/// string `model_no` used by `PlcModelIn`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeIn {
    pub edge_id: String,
    pub model_no: i64,
    pub model_name: String,
    pub levels: i64,
    pub recipe: RecipeSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherCurrent {
    pub location_name: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub observed_at: DateTime<FixedOffset>,
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub source: String,
}
