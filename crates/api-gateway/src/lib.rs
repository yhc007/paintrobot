//! Paintrobot API gateway — compiled to wasm32-wasip2 and served by `wasmtime serve`.
//!
//! Implemented routes:
//!   POST /api/v1/jobs            — ingest one matched job from the edge
//!   GET  /api/v1/stats/today     — today's per-model counts (aggregated from CoreDB)
//!   GET  /api/v1/weather/current — current °C / %RH at 현대정밀 (stub until weather-client wired)
//!   GET  /healthz                — liveness

pub mod config;

use chrono::{Duration, NaiveDate, Utc};
use http_body_util::BodyExt;
use paintrobot_domain as domain;
use paintrobot_repo_coredb::{
    CoatingRow, CoreDbClient, JobRow, RecipeRow, RepoError, WasiTransport, WeatherRow,
};
use paintrobot_schema::{
    CoatingIn, CoatingOut, DailyStats, IngestResponse, JobIn, PlcCurrent, PlcModelIn,
    PressureFactors, RecipeIn, Rejected, WeatherCurrent,
};
use paintrobot_weather_client::{OwmProvider, WeatherError, WeatherProvider};
use wstd::http::{Body, Request, Response, StatusCode};

fn client() -> CoreDbClient<WasiTransport> {
    CoreDbClient::new(WasiTransport::new(), config::coredb_url(), config::coredb_keyspace())
}

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>, wstd::http::Error> {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let query = req.uri().query().unwrap_or("").to_owned();
    match (method.as_str(), path.as_str()) {
        ("POST", "/api/v1/jobs") => Ok(ingest_job(req).await),
        ("POST", "/api/v1/plc/model") => Ok(ingest_plc_model(req).await),
        ("GET", "/api/v1/plc/current") => Ok(plc_current().await),
        ("POST", "/api/v1/plc/recipe") => Ok(ingest_recipe(req).await),
        ("GET", "/api/v1/plc/recipe/current") => Ok(recipe_current(&query).await),
        ("POST", "/api/v1/coatings") => Ok(ingest_coating(req).await),
        ("GET", "/api/v1/coatings/today") => Ok(coatings_today().await),
        ("GET", "/api/v1/coatings/recent") => Ok(coatings_recent(&query).await),
        ("GET", "/api/v1/stats/today") => Ok(stats_today().await),
        ("GET", "/api/v1/stats/daily") => Ok(stats_daily(&query).await),
        ("GET", "/api/v1/stats/range") => Ok(stats_range(&query).await),
        ("GET", "/api/v1/stats/bounds") => Ok(stats_bounds().await),
        ("GET", "/api/v1/jobs") => Ok(list_jobs(&query).await),
        ("GET", "/api/v1/jobs/export.csv") => Ok(export_jobs_csv(&query).await),
        ("GET", "/api/v1/weather/current") => Ok(weather_current().await),
        ("GET", "/api/v1/stream/live") => Ok(stream_live()),
        ("GET", "/healthz") => Ok(text(StatusCode::OK, "ok")),
        _ => Ok(text(StatusCode::NOT_FOUND, "not found")),
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

fn text(status: StatusCode, msg: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from(msg.to_string()))
        .expect("response build")
}

fn json_response<T: serde::Serialize>(status: StatusCode, value: &T) -> Response<Body> {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("response build")
}

fn json_error(status: StatusCode, reason: &str) -> Response<Body> {
    json_response(
        status,
        &serde_json::json!({ "error": reason }),
    )
}

async fn read_body(req: Request<Body>) -> Result<Vec<u8>, String> {
    let collected = req
        .into_body()
        .into_boxed_body()
        .collect()
        .await
        .map_err(|e| format!("read body: {e}"))?;
    Ok(collected.to_bytes().to_vec())
}

fn check_edge_key(headers: &wstd::http::HeaderMap) -> bool {
    let configured = config::edge_api_keys();
    if configured.is_empty() {
        // No keys configured ⇒ open mode (useful for local dev). Log warning at startup instead.
        return true;
    }
    let Some(given) = headers.get("x-edge-key").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    configured.iter().any(|k| k == given)
}

// ── routes ─────────────────────────────────────────────────────────────────

async fn ingest_job(req: Request<Body>) -> Response<Body> {
    if !check_edge_key(req.headers()) {
        return json_error(StatusCode::UNAUTHORIZED, "missing or invalid X-Edge-Key");
    }

    let body = match read_body(req).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let job: JobIn = match serde_json::from_slice(&body) {
        Ok(j) => j,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("invalid body: {e}")),
    };

    let status = domain::classify(&job);
    let work_date_src = job
        .plc_ts
        .or(job.camera_ts)
        .unwrap_or_else(|| Utc::now().fixed_offset());
    let work_date = domain::work_date(work_date_src, config::kst());
    let created_at = Utc::now().timestamp_millis();

    let c = client();
    match c.get_job(&job.event_id).await {
        Ok(Some(_)) => {
            return json_response(
                StatusCode::OK,
                &IngestResponse {
                    accepted: 0,
                    duplicates: 1,
                    rejected: vec![],
                },
            );
        }
        Ok(None) => {}
        Err(e) => return repo_error_response(&e),
    }

    match c.insert_job(&job, &work_date, status, created_at).await {
        Ok(()) => json_response(
            StatusCode::OK,
            &IngestResponse {
                accepted: 1,
                duplicates: 0,
                rejected: vec![],
            },
        ),
        Err(RepoError::InvalidIdentifier(bad)) => json_response(
            StatusCode::OK,
            &IngestResponse {
                accepted: 0,
                duplicates: 0,
                rejected: vec![Rejected {
                    event_id: job.event_id,
                    reason: format!("invalid identifier: {bad}"),
                }],
            },
        ),
        Err(e) => repo_error_response(&e),
    }
}

// ── plc state ──────────────────────────────────────────────────────────────

async fn ingest_plc_model(req: Request<Body>) -> Response<Body> {
    if !check_edge_key(req.headers()) {
        return json_error(StatusCode::UNAUTHORIZED, "missing or invalid X-Edge-Key");
    }
    let body = match read_body(req).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let inp: PlcModelIn = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("invalid body: {e}")),
    };

    let plc_ts = inp.plc_ts.unwrap_or_else(|| Utc::now().fixed_offset());
    // Deterministic event_id from (edge, model, ts) so the same state update
    // re-sent within the same millisecond is idempotent.
    let event_id = format!(
        "plc-{}-{}-{}",
        sanitize_id(&inp.edge_id),
        sanitize_id(&inp.model_no),
        plc_ts.timestamp_millis()
    );
    let job = JobIn {
        event_id: event_id.clone(),
        edge_id: inp.edge_id.clone(),
        plc_model_no: Some(inp.model_no.clone()),
        camera_model_no: None,
        plc_ts: Some(plc_ts),
        camera_ts: None,
        confidence: None,
        image_ref: None,
    };

    let status = domain::classify(&job);
    let work_date = domain::work_date(plc_ts, config::kst());
    let created_at = Utc::now().timestamp_millis();

    let c = client();
    match c.get_job(&event_id).await {
        Ok(Some(_)) => {
            return json_response(
                StatusCode::OK,
                &serde_json::json!({
                    "accepted": 0,
                    "duplicates": 1,
                    "current_model": inp.model_no,
                    "event_id": event_id,
                }),
            );
        }
        Ok(None) => {}
        Err(e) => return repo_error_response(&e),
    }

    match c.insert_job(&job, &work_date, status, created_at).await {
        Ok(()) => json_response(
            StatusCode::OK,
            &serde_json::json!({
                "accepted": 1,
                "duplicates": 0,
                "current_model": inp.model_no,
                "event_id": event_id,
            }),
        ),
        Err(e) => repo_error_response(&e),
    }
}

/// Pick the most recent PLC-side event from today's jobs.
async fn plc_current() -> Response<Body> {
    let cur = match latest_plc_state().await {
        Ok(c) => c,
        Err(e) => return repo_error_response(&e),
    };
    json_response(StatusCode::OK, &cur)
}

async fn latest_plc_state() -> Result<PlcCurrent, RepoError> {
    let today = Utc::now()
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();
    let rows = client().scan_jobs_for_date(&today, 100_000).await?;
    Ok(latest_plc_state_from_rows(&rows))
}

fn latest_plc_state_from_rows(rows: &[paintrobot_repo_coredb::JobRow]) -> PlcCurrent {
    // Latest PLC reading (any row with a non-empty plc_model_no).
    let latest_plc = rows
        .iter()
        .filter(|r| r.plc_model_no.as_deref().filter(|s| !s.is_empty()).is_some())
        .max_by_key(|r| r.plc_ts.unwrap_or(r.created_at));
    // Latest camera reading (any row with a non-empty camera_model_no).
    let latest_cam = rows
        .iter()
        .filter(|r| {
            r.camera_model_no
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_some()
        })
        .max_by_key(|r| r.camera_ts.unwrap_or(r.created_at));

    PlcCurrent {
        model_no: latest_plc.and_then(|r| r.plc_model_no.clone()),
        edge_id: latest_plc.map(|r| r.edge_id.clone()),
        plc_ts: latest_plc.and_then(|r| r.plc_ts),
        event_id: latest_plc.map(|r| r.event_id.clone()),
        camera_model_no: latest_cam.and_then(|r| r.camera_model_no.clone()),
        camera_ts: latest_cam.and_then(|r| r.camera_ts),
    }
}

/// Replace whitespace and invalid chars with `_` so check_identifier passes.
fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ':' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

// ── paint recipe ─────────────────────────────────────────────────────────────

async fn ingest_recipe(req: Request<Body>) -> Response<Body> {
    if !check_edge_key(req.headers()) {
        return json_error(StatusCode::UNAUTHORIZED, "missing or invalid X-Edge-Key");
    }
    let body = match read_body(req).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let inp: RecipeIn = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("invalid body: {e}")),
    };

    // Every parameter's table/applied array length must equal `levels`.
    if inp.levels <= 0 || inp.levels > 256 {
        return json_error(StatusCode::BAD_REQUEST, "levels out of range (1..=256)");
    }
    let n = inp.levels as usize;
    for (name, p) in [
        ("atomization", &inp.recipe.atomization),
        ("pattern", &inp.recipe.pattern),
        ("flow", &inp.recipe.flow),
    ] {
        if p.table.len() != n || p.applied.len() != n {
            return json_error(
                StatusCode::BAD_REQUEST,
                &format!("{name}: table/applied length must equal levels ({n})"),
            );
        }
    }

    // Compact (space-free, ASCII) JSON — safe to embed as a CQL text literal.
    let recipe_json = match serde_json::to_string(&inp.recipe) {
        Ok(s) => s,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("serialize recipe: {e}")),
    };

    let now = Utc::now();
    let received_at = now.timestamp_millis();
    let work_date = now
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();
    // Idempotent upsert: one row per (edge, model_no). Re-posting (polling) the
    // same model overwrites with the latest recipe — CoreDB upserts by PK.
    let event_id = format!("recipe-{}-{}", sanitize_id(&inp.edge_id), inp.model_no);

    let row = RecipeRow {
        event_id: event_id.clone(),
        edge_id: inp.edge_id.clone(),
        model_no: inp.model_no,
        model_name: inp.model_name.clone(),
        levels: inp.levels,
        recipe_json,
        received_at,
        work_date,
    };

    match client().insert_recipe(&row).await {
        Ok(()) => json_response(
            StatusCode::OK,
            &serde_json::json!({
                "result": "ok",
                "event_id": event_id,
                "model_no": inp.model_no,
                "model_name": inp.model_name,
            }),
        ),
        Err(e) => repo_error_response(&e),
    }
}

/// Most recent recipe posted today (optionally filtered by `?edge_id=`).
async fn recipe_current(query: &str) -> Response<Body> {
    let want_edge = query_param(query, "edge_id");
    let today = Utc::now()
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();
    let rows = match client().scan_recipes_for_date(&today, 100_000).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };
    let latest = rows
        .iter()
        .filter(|r| match &want_edge {
            Some(e) => &r.edge_id == e,
            None => true,
        })
        .max_by_key(|r| r.received_at);

    match latest {
        Some(r) => {
            let recipe: serde_json::Value =
                serde_json::from_str(&r.recipe_json).unwrap_or(serde_json::Value::Null);
            json_response(
                StatusCode::OK,
                &serde_json::json!({
                    "edge_id": r.edge_id,
                    "model_no": r.model_no,
                    "model_name": r.model_name,
                    "levels": r.levels,
                    "recipe": recipe,
                    "received_at": r.received_at,
                    "work_date": r.work_date,
                }),
            )
        }
        None => json_response(
            StatusCode::OK,
            &serde_json::json!({ "model_no": serde_json::Value::Null, "recipe": serde_json::Value::Null }),
        ),
    }
}

// ── coatings ───────────────────────────────────────────────────────────────

async fn ingest_coating(req: Request<Body>) -> Response<Body> {
    if !check_edge_key(req.headers()) {
        return json_error(StatusCode::UNAUTHORIZED, "missing or invalid X-Edge-Key");
    }
    let body = match read_body(req).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let inp: CoatingIn = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("invalid body: {e}")),
    };

    // Resolve temperature/humidity. Edge can supply, otherwise pull live OWM.
    let (temperature_c, humidity_pct) = match (inp.temperature_c, inp.humidity_pct) {
        (Some(t), Some(h)) => (t, h),
        _ => match owm_now().await {
            Ok((t, h)) => (
                inp.temperature_c.unwrap_or(t),
                inp.humidity_pct.unwrap_or(h),
            ),
            Err(_) => (
                inp.temperature_c.unwrap_or(20.0),
                inp.humidity_pct.unwrap_or(50.0),
            ),
        },
    };

    let target = inp.target_um.unwrap_or(domain::DEFAULT_TARGET_UM);
    let calc = domain::recommend_pressure(
        inp.measured_um,
        target,
        temperature_c,
        humidity_pct,
        inp.current_pressure,
    );

    let now = Utc::now();
    let measured_at = now.timestamp_millis();
    let work_date = now
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();

    let row = CoatingRow {
        event_id: inp.event_id.clone(),
        job_event_id: inp.job_event_id.clone(),
        model_no: inp.model_no.clone(),
        measured_um: inp.measured_um,
        target_um: target,
        temperature_c,
        humidity_pct,
        current_pressure: inp.current_pressure,
        recommended_pressure: calc.recommended,
        thickness_error: calc.thickness_error,
        control_factor: calc.factors.control,
        temp_factor: calc.factors.temperature,
        humidity_factor: calc.factors.humidity,
        measured_at,
        work_date: work_date.clone(),
    };

    if let Err(e) = client().insert_coating(&row).await {
        // The recommendation is still useful even if persistence failed; log via response.
        let mut out = build_coating_out(&row);
        let resp = serde_json::json!({
            "coating": out_value(&mut out),
            "stored": false,
            "store_error": format!("{e}")
        });
        return json_response(StatusCode::OK, &resp);
    }

    json_response(StatusCode::OK, &build_coating_out(&row))
}

fn build_coating_out(row: &CoatingRow) -> CoatingOut {
    CoatingOut {
        event_id: row.event_id.clone(),
        model_no: row.model_no.clone(),
        measured_um: row.measured_um,
        target_um: row.target_um,
        current_pressure: row.current_pressure,
        recommended_pressure: row.recommended_pressure,
        thickness_error: row.thickness_error,
        temperature_c: row.temperature_c,
        humidity_pct: row.humidity_pct,
        factors: PressureFactors {
            control: row.control_factor,
            temperature: row.temp_factor,
            humidity: row.humidity_factor,
        },
        measured_at: row.measured_at,
        work_date: row.work_date.clone(),
    }
}

fn out_value(out: &mut CoatingOut) -> serde_json::Value {
    serde_json::to_value(&*out).unwrap_or(serde_json::Value::Null)
}

async fn coatings_today() -> Response<Body> {
    let today = Utc::now()
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();
    let rows = match client().scan_coatings_for_date(&today, 100_000).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };
    coatings_response(rows, &today)
}

async fn coatings_recent(query: &str) -> Response<Body> {
    let limit: usize = query_param(query, "limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50)
        .min(2000);
    let today = Utc::now()
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();
    let mut rows = match client().scan_coatings_for_date(&today, 100_000).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };
    rows.sort_by_key(|r| std::cmp::Reverse(r.measured_at));
    rows.truncate(limit);
    rows.reverse(); // ascending for charts
    coatings_response(rows, &today)
}

fn coatings_response(rows: Vec<CoatingRow>, work_date: &str) -> Response<Body> {
    let total = rows.len();
    let avg_measured = if total > 0 {
        rows.iter().map(|r| r.measured_um).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let avg_recommended = if total > 0 {
        rows.iter().map(|r| r.recommended_pressure).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let series: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "event_id": r.event_id,
                "model_no": r.model_no,
                "measured_um": r.measured_um,
                "target_um": r.target_um,
                "current_pressure": r.current_pressure,
                "recommended_pressure": r.recommended_pressure,
                "temperature_c": r.temperature_c,
                "humidity_pct": r.humidity_pct,
                "measured_at": r.measured_at,
            })
        })
        .collect();
    json_response(
        StatusCode::OK,
        &serde_json::json!({
            "work_date": work_date,
            "total": total,
            "avg_measured_um": avg_measured,
            "avg_recommended_pressure": avg_recommended,
            "series": series,
        }),
    )
}

async fn owm_now() -> Result<(f64, f64), WeatherError> {
    let key = config::owm_api_key().ok_or(WeatherError::MissingKey)?;
    let provider = OwmProvider::new(key);
    let w = provider.current(config::SITE_LAT, config::SITE_LON).await?;
    Ok((w.temperature_c, w.humidity_pct))
}

async fn stats_today() -> Response<Body> {
    let today = Utc::now()
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();
    stats_for_date(&today).await
}

async fn stats_daily(query: &str) -> Response<Body> {
    let Some(date) = query_param(query, "date") else {
        return json_error(StatusCode::BAD_REQUEST, "missing date");
    };
    if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        return json_error(StatusCode::BAD_REQUEST, "date must be YYYY-MM-DD");
    }
    stats_for_date(&date).await
}

async fn stats_for_date(date: &str) -> Response<Body> {
    let c = client();
    match c.agg_rows_for_date(date, 100_000).await {
        Ok(rows) => {
            let stats: DailyStats = domain::aggregate(date.to_string(), rows);
            json_response(StatusCode::OK, &stats)
        }
        Err(e) => repo_error_response(&e),
    }
}

async fn stats_range(query: &str) -> Response<Body> {
    let Some(from) = query_param(query, "from") else {
        return json_error(StatusCode::BAD_REQUEST, "missing from");
    };
    let Some(to) = query_param(query, "to") else {
        return json_error(StatusCode::BAD_REQUEST, "missing to");
    };
    let group_by = query_param(query, "group_by").unwrap_or_else(|| "day".to_string());

    let (Ok(mut f), Ok(mut t)) = (
        NaiveDate::parse_from_str(&from, "%Y-%m-%d"),
        NaiveDate::parse_from_str(&to, "%Y-%m-%d"),
    ) else {
        return json_error(StatusCode::BAD_REQUEST, "from/to must be YYYY-MM-DD");
    };
    if t < f {
        std::mem::swap(&mut f, &mut t);
    }

    let c = client();

    // 구간이 상한을 넘어도 거절하지 않는다. 사용자가 달력에서 넉넉하게 집은
    // 구간을 400으로 되돌려주면 화면이 그냥 죽는다 — 대신 실제 데이터가 있는
    // 쪽으로 당겨서 되돌려주고, 어디까지 집계됐는지는 응답의 work_date가 말해준다.
    if span_days(f, t) > MAX_RANGE_DAYS {
        if let Ok(Some((first, last))) = c.job_date_bounds(1_000_000).await {
            if let Ok(bf) = NaiveDate::parse_from_str(&first, "%Y-%m-%d") {
                f = f.max(bf);
            }
            if let Ok(bl) = NaiveDate::parse_from_str(&last, "%Y-%m-%d") {
                t = t.min(bl);
            }
        }
        // 보유 구간 자체가 상한보다 넓으면 최근 쪽을 남긴다.
        if span_days(f, t) > MAX_RANGE_DAYS {
            f = t - Duration::days(MAX_RANGE_DAYS - 1);
        }
    }

    // 요청 구간과 보유 구간이 아예 안 겹치면 빈 결과다 — 에러가 아니다.
    if t < f {
        return match group_by.as_str() {
            "day" => json_response(StatusCode::OK, &Vec::<DailyStats>::new()),
            "model" => json_response(StatusCode::OK, &sum_by_model(&[])),
            _ => json_error(StatusCode::BAD_REQUEST, "group_by must be day|model"),
        };
    }

    let from = f.format("%Y-%m-%d").to_string();
    let to = t.format("%Y-%m-%d").to_string();
    let dates = match iter_dates(&from, &to) {
        Ok(d) => d,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };

    // One ranged read, not one per day: CoreDB scans the whole `jobs` table for
    // every statement, so N per-day queries cost N full scans (a 7-day window
    // took ~24s). Bucket the rows here instead.
    use std::collections::BTreeMap;
    let rows = match c.agg_rows_for_range(&from, &to, 1_000_000).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };
    let mut by_date: BTreeMap<String, Vec<domain::AggRow>> = BTreeMap::new();
    for (d, row) in rows {
        by_date.entry(d).or_default().push(row);
    }
    // Every requested day is reported, including the ones CoreDB had nothing for.
    let daily: Vec<DailyStats> = dates
        .iter()
        .map(|d| domain::aggregate(d.clone(), by_date.remove(d).unwrap_or_default()))
        .collect();

    match group_by.as_str() {
        "day" => json_response(StatusCode::OK, &daily),
        "model" => json_response(StatusCode::OK, &sum_by_model(&daily)),
        _ => json_error(StatusCode::BAD_REQUEST, "group_by must be day|model"),
    }
}

/// First and last work_date that actually carry counted jobs.
///
/// The dashboard calls this when a chosen window came back empty, so it can move
/// the range onto the data instead of showing a blank chart.
async fn stats_bounds() -> Response<Body> {
    let c = client();
    match c.job_date_bounds(1_000_000).await {
        Ok(Some((first, last))) => json_response(
            StatusCode::OK,
            &serde_json::json!({ "first_date": first, "last_date": last }),
        ),
        Ok(None) => json_response(
            StatusCode::OK,
            &serde_json::json!({ "first_date": null, "last_date": null }),
        ),
        Err(e) => repo_error_response(&e),
    }
}

fn sum_by_model(daily: &[DailyStats]) -> Vec<paintrobot_schema::ModelCount> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for d in daily {
        for m in &d.models {
            let e = acc.entry(m.model_no.clone()).or_insert((0, 0));
            e.0 += m.job_count;
            e.1 += m.mismatch_count;
        }
    }
    acc.into_iter()
        .map(|(model_no, (job_count, mismatch_count))| paintrobot_schema::ModelCount {
            model_no,
            job_count,
            mismatch_count,
        })
        .collect()
}

async fn list_jobs(query: &str) -> Response<Body> {
    let from = query_param(query, "from");
    let to = query_param(query, "to");
    let model = query_param(query, "model");
    let status = query_param(query, "status");
    let page: usize = query_param(query, "page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let per_page: usize = query_param(query, "per_page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
        .min(2000);

    let rows = match collect_jobs(from.as_deref(), to.as_deref()).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };

    let filtered: Vec<&JobRow> = rows
        .iter()
        .filter(|r| match &model {
            Some(m) => r.plc_model_no.as_deref() == Some(m.as_str())
                || r.camera_model_no.as_deref() == Some(m.as_str()),
            None => true,
        })
        .filter(|r| match &status {
            Some(s) => &r.match_status == s,
            None => true,
        })
        .collect();

    let total = filtered.len();
    let start = page.saturating_mul(per_page).min(total);
    let end = (start + per_page).min(total);
    let page_rows: Vec<serde_json::Value> = filtered[start..end]
        .iter()
        .map(|r| job_row_json(r))
        .collect();

    json_response(
        StatusCode::OK,
        &serde_json::json!({
            "total": total,
            "page": page,
            "per_page": per_page,
            "rows": page_rows,
        }),
    )
}

async fn export_jobs_csv(query: &str) -> Response<Body> {
    let from = query_param(query, "from");
    let to = query_param(query, "to");
    let model = query_param(query, "model");
    let status = query_param(query, "status");

    let rows = match collect_jobs(from.as_deref(), to.as_deref()).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };

    let mut out = String::from(
        "work_date,event_id,edge_id,plc_model_no,camera_model_no,match_status,plc_ts,camera_ts,confidence\n",
    );
    for r in rows.iter().filter(|r| match &model {
        Some(m) => r.plc_model_no.as_deref() == Some(m.as_str())
            || r.camera_model_no.as_deref() == Some(m.as_str()),
        None => true,
    }).filter(|r| match &status {
        Some(s) => &r.match_status == s,
        None => true,
    }) {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            r.work_date,
            csv_escape(&r.event_id),
            csv_escape(&r.edge_id),
            csv_escape(r.plc_model_no.as_deref().unwrap_or("")),
            csv_escape(r.camera_model_no.as_deref().unwrap_or("")),
            r.match_status,
            r.plc_ts.unwrap_or(0),
            r.camera_ts.unwrap_or(0),
            r.confidence.unwrap_or(0.0),
        ));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/csv; charset=utf-8")
        .header(
            "content-disposition",
            format!(
                "attachment; filename=paintrobot_{}_{}.csv",
                from.as_deref().unwrap_or("all"),
                to.as_deref().unwrap_or("today")
            ),
        )
        .body(Body::from(out))
        .expect("response build")
}

async fn collect_jobs(
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Vec<JobRow>, RepoError> {
    let (from, to) = match (from, to) {
        (Some(f), Some(t)) => (f.to_string(), t.to_string()),
        _ => {
            let today = Utc::now()
                .with_timezone(&config::kst())
                .format("%Y-%m-%d")
                .to_string();
            (today.clone(), today)
        }
    };
    let dates = iter_dates(&from, &to).map_err(RepoError::Db)?;
    if dates.len() > 366 {
        return Err(RepoError::Db("range exceeds 366 days".into()));
    }
    let c = client();
    let mut all = Vec::new();
    for d in &dates {
        let rows = c.scan_jobs_for_date(d, 100_000).await?;
        all.extend(rows);
    }
    Ok(all)
}

fn job_row_json(r: &JobRow) -> serde_json::Value {
    serde_json::json!({
        "work_date": r.work_date,
        "event_id": r.event_id,
        "edge_id": r.edge_id,
        "plc_model_no": r.plc_model_no,
        "camera_model_no": r.camera_model_no,
        "plc_ts": r.plc_ts,
        "camera_ts": r.camera_ts,
        "confidence": r.confidence,
        "match_status": r.match_status,
        "image_ref": r.image_ref,
    })
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 한 번의 조회로 훑을 수 있는 최대 일수. 넘으면 거절하지 않고 잘라낸다.
const MAX_RANGE_DAYS: i64 = 366;

fn span_days(from: NaiveDate, to: NaiveDate) -> i64 {
    (to - from).num_days() + 1
}

fn iter_dates(from: &str, to: &str) -> Result<Vec<String>, String> {
    let f = NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|e| format!("bad from: {e}"))?;
    let t = NaiveDate::parse_from_str(to, "%Y-%m-%d")
        .map_err(|e| format!("bad to: {e}"))?;
    if t < f {
        return Err("to must be >= from".into());
    }
    let mut out = Vec::new();
    let mut d = f;
    while d <= t {
        out.push(d.format("%Y-%m-%d").to_string());
        d += Duration::days(1);
    }
    Ok(out)
}

fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next()?;
        let v = it.next().unwrap_or("");
        if k == key {
            return Some(url_decode(v));
        }
    }
    None
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(byte) = u8::from_str_radix(
                    std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00"),
                    16,
                ) {
                    out.push(byte);
                }
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

async fn weather_current() -> Response<Body> {
    let Some(key) = config::owm_api_key() else {
        // No key configured — return a clearly-tagged stub.
        let now = Utc::now().fixed_offset();
        return json_response(
            StatusCode::OK,
            &WeatherCurrent {
                location_name: config::SITE_NAME,
                lat: config::SITE_LAT,
                lon: config::SITE_LON,
                observed_at: now,
                temperature_c: 0.0,
                humidity_pct: 0.0,
                source: "stub".into(),
            },
        );
    };

    let provider = OwmProvider::new(key);
    match provider.current(config::SITE_LAT, config::SITE_LON).await {
        Ok(w) => {
            // Best-effort persistence to paintrobot.weather_snapshots.
            // observed_at must avoid '+' so it passes check_identifier; use UTC Z form.
            let observed_at = w
                .observed_at
                .with_timezone(&Utc)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            let row = WeatherRow {
                observed_at,
                temperature_c: w.temperature_c,
                humidity_pct: w.humidity_pct,
                source: "owm".to_string(),
            };
            let _ = client().insert_weather(&row).await;
            json_response(
                StatusCode::OK,
                &WeatherCurrent {
                    location_name: config::SITE_NAME,
                    lat: config::SITE_LAT,
                    lon: config::SITE_LON,
                    observed_at: w.observed_at,
                    temperature_c: w.temperature_c,
                    humidity_pct: w.humidity_pct,
                    source: w.source.to_string(),
                },
            )
        }
        Err(e) => weather_error_response(&e),
    }
}

fn weather_error_response(e: &WeatherError) -> Response<Body> {
    let (status, msg) = match e {
        WeatherError::MissingKey => (StatusCode::SERVICE_UNAVAILABLE, "owm key missing".to_string()),
        WeatherError::Transport(s) => (StatusCode::BAD_GATEWAY, format!("owm transport: {s}")),
        WeatherError::Decode(s) => (StatusCode::BAD_GATEWAY, format!("owm decode: {s}")),
        WeatherError::Upstream(c, b) => (StatusCode::BAD_GATEWAY, format!("owm {c}: {b}")),
    };
    json_error(status, &msg)
}

/// Server-Sent Events stream that emits today's stats every 5 seconds.
/// Closes after ~1 hour; clients auto-reconnect.
fn stream_live() -> Response<Body> {
    use futures_lite::stream::unfold;
    use std::convert::Infallible;
    use wstd::http::body::Bytes;
    use wstd::time::Duration;

    const INTERVAL_SECS: u64 = 2;
    const MAX_ITERS: u32 = 1800; // ~1h

    let stream = unfold(MAX_ITERS, |iters| async move {
        if iters == 0 {
            return None;
        }
        if iters < MAX_ITERS {
            wstd::task::sleep(Duration::from_secs(INTERVAL_SECS)).await;
        }
        let today = Utc::now()
            .with_timezone(&config::kst())
            .format("%Y-%m-%d")
            .to_string();
        let payload = match client().scan_jobs_for_date(&today, 100_000).await {
            Ok(rows) => {
                let plc = latest_plc_state_from_rows(&rows);
                let agg_rows: Vec<paintrobot_domain::AggRow> = rows
                    .iter()
                    .map(|r| paintrobot_domain::AggRow {
                        model_no: r
                            .plc_model_no
                            .clone()
                            .filter(|s| !s.is_empty())
                            .or_else(|| r.camera_model_no.clone().filter(|s| !s.is_empty()))
                            .unwrap_or_else(|| "(unknown)".to_string()),
                        match_status: r.match_status.clone(),
                    })
                    .collect();
                let stats = domain::aggregate(today, agg_rows);
                serde_json::to_string(&serde_json::json!({
                    "stats": stats,
                    "current_plc": plc,
                }))
                .unwrap_or_else(|_| "{}".to_string())
            }
            Err(_) => "{}".to_string(),
        };
        let frame = format!("event: stats\ndata: {payload}\n\n");
        Some((Ok::<_, Infallible>(Bytes::from(frame)), iters - 1))
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(Body::from_try_stream(stream))
        .expect("response build")
}

fn repo_error_response(e: &RepoError) -> Response<Body> {
    match e {
        RepoError::InvalidIdentifier(s) => {
            json_error(StatusCode::BAD_REQUEST, &format!("invalid identifier: {s}"))
        }
        RepoError::Transport(s) => {
            json_error(StatusCode::BAD_GATEWAY, &format!("coredb transport: {s}"))
        }
        RepoError::Decode(s) => {
            json_error(StatusCode::BAD_GATEWAY, &format!("coredb decode: {s}"))
        }
        RepoError::Db(s) => json_error(StatusCode::BAD_GATEWAY, &format!("coredb error: {s}")),
    }
}
