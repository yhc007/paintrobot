//! DTOs shared between edge clients, API gateway, and the frontend.

use std::collections::BTreeMap;

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

// ── R1 로봇(MPX2600) 인터페이스 ────────────────────────────────────────────
//
// `POST /api/v1/plc/robot` 의 페이로드. 스펙은 `plc_r.md`.
//
// 여기 오는 것은 도장 수치가 아니라 **핸드셰이크와 추적** 데이터다. PLC는
// 도장 파라미터(무화/패턴/토출량)를 로봇에 넘기지 않는다 — D/A 출력으로
// 도장건을 직접 물린다. 로봇에는 "몇 번 차종인가"(WORK ID)와 기동/모드
// 지령 비트만 접점으로 간다. 그래서 이 엔드포인트가 답하는 질문은
// "몇 번 차체가, 어느 JIG에서, 언제 로봇에 넘어갔나" 이다.

/// 비트 묶음. **`true`/`false`/`None` 3-state를 반드시 유지한다.**
///
/// `None`은 "해당 워드 블록 읽기 실패 = 값 모름"이지 `false`가 아니다.
/// 비상정지 비트가 `None`인데 `false`로 접히면 화면이 "안전함"으로 읽힌다.
/// 그래서 `Option<bool>`이고, 맵이라 스펙에 없는 키가 와도 그대로 실린다
/// (스펙 §2: forward-compatible).
pub type BitMap = BTreeMap<String, Option<bool>>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RobotIo {
    /// PLC → 로봇 지령 (내부 릴레이 M)
    #[serde(default)]
    pub command: BitMap,
    /// 로봇 운전 상태
    #[serde(default)]
    pub state: BitMap,
    /// 로봇 알람
    #[serde(default)]
    pub alarm: BitMap,
    /// PLC → 로봇 출력 접점 (P 영역, 실제 배선)
    ///
    /// `command`와 키 이름이 겹친다(`external_start`, `servo_on` 등). 전자는
    /// 내부 릴레이, 후자는 실제 출력 접점이라 의미가 다르므로 절대 합치지
    /// 않는다. 둘이 어긋나면 출력 모듈/배선 이상 신호다.
    #[serde(default, rename = "do")]
    pub out: BitMap,
    /// 로봇 → PLC 입력 접점 (P 영역)
    #[serde(default)]
    pub di: BitMap,
}

/// JIG 5단 시프트 레지스터의 한 칸. `no`는 1~5.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JigStation {
    pub no: i64,
    /// 이 JIG에 실린 차체의 차종번호. 컨베이어를 따라 차체와 같이 움직인다.
    #[serde(default)]
    pub work_id: Option<i64>,
    /// 0 = 워크 없음, 그 외 = 있음
    #[serde(default)]
    pub work_in: Option<i64>,
    #[serde(default)]
    pub shift_dist: Option<i64>,
    #[serde(default)]
    pub job_start_dist: Option<i64>,
    #[serde(default)]
    pub send_to_robot: Option<bool>,
    #[serde(default)]
    pub job_start: Option<bool>,
    #[serde(default)]
    pub send_done: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JigCompleted {
    #[serde(default)]
    pub work_id: Option<i64>,
    #[serde(default)]
    pub work_in: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JigIn {
    #[serde(default)]
    pub shift_distance: Option<i64>,
    #[serde(default)]
    pub chattering_guard: Option<i64>,
    #[serde(default)]
    pub start_sig_start_dist: Option<i64>,
    #[serde(default)]
    pub start_sig_end_dist: Option<i64>,
    #[serde(default)]
    pub common: BitMap,
    /// 항상 5개, `no` 오름차순.
    #[serde(default)]
    pub stations: Vec<JigStation>,
    #[serde(default)]
    pub completed: JigCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotIn {
    pub edge_id: String,
    #[serde(default)]
    pub robot_model: Option<String>,
    /// HMI 차종번호 %DW5000. 1~8 밖이어도 거절하지 않는다 — 미입력/전환 중에
    /// 0이나 범위 밖 값이 실제로 나온다 (스펙 §4.2).
    #[serde(default)]
    pub model_no: Option<i64>,
    #[serde(default)]
    pub robot: RobotIo,
    #[serde(default)]
    pub jig: JigIn,
    /// 비었으면 전 구간 정상. 5건이면 PLC 연결 자체가 끊긴 상태.
    #[serde(default)]
    pub read_errors: Vec<String>,
}

/// 스펙 §2/§4.2에서 요구하는 값 규칙 검사. 통과 못 하면 400.
impl RobotIn {
    pub fn validate(&self) -> Result<(), String> {
        if self.edge_id.trim().is_empty() {
            return Err("edge_id must be a non-empty string".into());
        }
        if self.jig.stations.len() != JIG_STATIONS {
            return Err(format!(
                "jig.stations must have exactly {JIG_STATIONS} entries, got {}",
                self.jig.stations.len()
            ));
        }
        for (i, st) in self.jig.stations.iter().enumerate() {
            if st.no != (i + 1) as i64 {
                return Err(format!(
                    "jig.stations[{i}].no must be {}, got {}",
                    i + 1,
                    st.no
                ));
            }
        }
        // INT16 범위. serde가 이미 정수임은 보장하므로 범위만 본다.
        let ints: [(&str, Option<i64>); 4] = [
            ("jig.shift_distance", self.jig.shift_distance),
            ("jig.chattering_guard", self.jig.chattering_guard),
            ("jig.start_sig_start_dist", self.jig.start_sig_start_dist),
            ("jig.start_sig_end_dist", self.jig.start_sig_end_dist),
        ];
        for (name, v) in ints {
            check_int16(name, v)?;
        }
        for st in &self.jig.stations {
            let n = st.no;
            check_int16(&format!("jig.stations[{n}].work_id"), st.work_id)?;
            check_int16(&format!("jig.stations[{n}].work_in"), st.work_in)?;
            check_int16(&format!("jig.stations[{n}].shift_dist"), st.shift_dist)?;
            check_int16(
                &format!("jig.stations[{n}].job_start_dist"),
                st.job_start_dist,
            )?;
        }
        check_int16("jig.completed.work_id", self.jig.completed.work_id)?;
        check_int16("jig.completed.work_in", self.jig.completed.work_in)?;
        // model_no는 범위 밖이어도 통과시킨다 (§4.2). 경고는 화면이 붙인다.
        check_int16("model_no", self.model_no)?;
        Ok(())
    }

    /// `model_no`가 HMI 유효 범위(1~8) 밖 — 저장은 하되 경고 플래그를 세운다.
    pub fn model_no_out_of_range(&self) -> bool {
        !matches!(self.model_no, Some(1..=8))
    }
}

fn check_int16(name: &str, v: Option<i64>) -> Result<(), String> {
    match v {
        Some(n) if !(-32768..=32767).contains(&n) => {
            Err(format!("{name}: {n} is outside INT16 range"))
        }
        _ => Ok(()),
    }
}

pub const JIG_STATIONS: usize = 5;
