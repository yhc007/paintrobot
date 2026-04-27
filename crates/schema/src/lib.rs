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
