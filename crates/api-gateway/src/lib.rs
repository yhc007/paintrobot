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
        ("GET", "/api/v1/stats/mixflow") => Ok(stats_mixflow(&query).await),
        ("POST", "/api/v1/stats/rollup") => Ok(build_rollup(&query, req.headers()).await),
        // 관찰용(읽기 전용). 쓰기는 아래 POST + dry_run=false.
        ("GET", "/api/v1/stats/reconcile") => Ok(reconcile_jobs(&query, None).await),
        ("POST", "/api/v1/jobs/reconcile") => {
            Ok(reconcile_jobs(&query, Some(req.headers())).await)
        }
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
        Ok(()) => {
            // 카메라 인식도 plc_state에 반영해 `/plc/current`가 최신값을 준다.
            // 실패해도 수집 자체를 실패로 돌리지 않는다 — 여기는 파생 상태다.
            if let Some(cam) = job.camera_model_no.as_deref().filter(|m| !m.is_empty()) {
                let prev = c.get_plc_state(&job.edge_id).await.ok().flatten();
                let _ = c
                    .upsert_plc_state(&paintrobot_repo_coredb::PlcStateRow {
                        edge_id: job.edge_id.clone(),
                        model_no: prev.as_ref().and_then(|p| p.model_no.clone()),
                        plc_ts: prev.as_ref().and_then(|p| p.plc_ts),
                        camera_model_no: Some(cam.to_string()),
                        camera_ts: job.camera_ts.map(|t| t.timestamp_millis()),
                        updated_at: created_at,
                    })
                    .await;
            }
            json_response(
                StatusCode::OK,
                &IngestResponse {
                    accepted: 1,
                    duplicates: 0,
                    rejected: vec![],
                },
            )
        }
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

    // 엣지는 폴링할 때마다 이걸 보낸다. 모델이 그대로인데도 전부 `jobs`에
    // 넣으면 하루 7천여 행이 쌓인다 (실측 1,108:1 중복). 살아있음 표시는
    // plc_state 한 행에 덮어쓰고, `jobs`에는 실제 전환만 남긴다.
    let prev = c.get_plc_state(&inp.edge_id).await.ok().flatten();
    let changed = prev
        .as_ref()
        .and_then(|p| p.model_no.clone())
        .map(|m| m != inp.model_no)
        .unwrap_or(true);

    let _ = c
        .upsert_plc_state(&paintrobot_repo_coredb::PlcStateRow {
            edge_id: inp.edge_id.clone(),
            model_no: Some(inp.model_no.clone()),
            plc_ts: Some(plc_ts.timestamp_millis()),
            // 카메라 쪽은 건드리지 않는다 — 이전 값을 그대로 이어 쓴다.
            camera_model_no: prev.as_ref().and_then(|p| p.camera_model_no.clone()),
            camera_ts: prev.as_ref().and_then(|p| p.camera_ts),
            updated_at: created_at,
        })
        .await;

    if !changed {
        return json_response(
            StatusCode::OK,
            &serde_json::json!({
                "accepted": 0,
                "duplicates": 0,
                "unchanged": 1,
                "current_model": inp.model_no,
            }),
        );
    }

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
    let c = client();

    // plc_state는 엣지당 한 행이라 조회가 상수 시간이다. 예전에는 오늘치
    // `jobs`를 전부 훑어 최신값을 골랐는데, PLC 폴링 행이 그 스캔을 수천 배로
    //부풀렸다.
    //
    // 마이그레이션(006) 전에 이 빌드가 올라가도 죽지 않도록, 테이블이 없거나
    // 비어 있으면 예전 스캔으로 되돌아간다.
    if let Ok(states) = c.scan_plc_states(1_000).await {
        if let Some(latest) = states
            .iter()
            .max_by_key(|s| s.plc_ts.unwrap_or(s.updated_at))
        {
            return Ok(PlcCurrent {
                model_no: latest.model_no.clone(),
                edge_id: Some(latest.edge_id.clone()),
                plc_ts: latest.plc_ts,
                event_id: None,
                camera_model_no: latest.camera_model_no.clone(),
                camera_ts: latest.camera_ts,
            });
        }
    }

    let today = Utc::now()
        .with_timezone(&config::kst())
        .format("%Y-%m-%d")
        .to_string();
    let rows = c.scan_jobs_for_date(&today, 100_000).await?;
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
    // 날짜로 거르지 않는다. 레시피는 차종이 바뀔 때만 들어오므로, 당일분만
    // 보면 차종을 안 바꾼 날에는 화면이 빈다. 현장에서는 마지막으로 받은
    // 레시피가 계속 유효하다.
    let rows = match client().scan_all_recipes(100_000).await {
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
    if let Some(v) = rollup_part(date, "stats").await {
        return json_response(StatusCode::OK, &v);
    }
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

    // 미리 계산된 일별 집계가 있으면 그걸로 끝낸다. 롤업은 날짜당 한 행이라
    // 구간 조회가 수백 행 스캔에 그친다.
    if let Ok(rollups) = c.scan_rollups(&from, &to, 1_000).await {
        if !rollups.is_empty() {
            use std::collections::BTreeMap as RMap;
            let mut by_date: RMap<String, DailyStats> = RMap::new();
            for r in &rollups {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r.payload) {
                    if let Some(st) = v.get("stats") {
                        if let Ok(d) = serde_json::from_value::<DailyStats>(st.clone()) {
                            by_date.insert(r.work_date.clone(), d);
                        }
                    }
                }
            }
            if !by_date.is_empty() {
                // 요청한 날짜는 롤업이 없어도 0으로 채워 전부 응답한다.
                let daily: Vec<DailyStats> = dates
                    .iter()
                    .map(|d| {
                        by_date.remove(d).unwrap_or_else(|| {
                            domain::aggregate(d.clone(), Vec::new())
                        })
                    })
                    .collect();
                return match group_by.as_str() {
                    "day" => json_response(StatusCode::OK, &daily),
                    "model" => json_response(StatusCode::OK, &sum_by_model(&daily)),
                    _ => json_error(StatusCode::BAD_REQUEST, "group_by must be day|model"),
                };
            }
        }
    }

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

    // 롤업은 날짜당 한 행이라 전부 훑어도 수백 행이다. `jobs` 전체 스캔과
    // 비교할 바가 아니다. 집계가 0인 날은 경계로 치지 않는다 — 예전 구현과
    // 같은 기준이다.
    if let Ok(dates) = c.all_rollup_dates(10_000).await {
        let mut counted: Vec<String> = Vec::new();
        for d in dates {
            if let Some(v) = rollup_part(&d, "stats").await {
                if v.get("total_jobs").and_then(|n| n.as_u64()).unwrap_or(0) > 0 {
                    counted.push(d);
                }
            }
        }
        counted.sort();
        if let (Some(first), Some(last)) = (counted.first(), counted.last()) {
            return json_response(
                StatusCode::OK,
                &serde_json::json!({ "first_date": first, "last_date": last }),
            );
        }
    }

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

/// 미리 계산해둔 하루치 집계에서 한 조각을 꺼낸다.
///
/// 없으면 None — 호출측이 예전처럼 직접 계산한다. 롤업 타이머가 아직 안 돌았거나
/// 마이그레이션 전이어도 화면이 비지 않게 하기 위해서다.
async fn rollup_part(date: &str, key: &str) -> Option<serde_json::Value> {
    let r = client().get_rollup(date).await.ok()??;
    let v: serde_json::Value = serde_json::from_str(&r.payload).ok()?;
    v.get(key).cloned()
}

/// 하루치 행에서 지연 상관의 입력 두 개를 만든다.
///
/// 대상: 카메라가 본 차 중 PLC 타임스탬프가 없는 행. `camera_only`뿐 아니라
/// 이 배치가 이미 판정한 행도 다시 집는다 — 엣지가 직접 짝지어 보낸 행은
/// `plc_ts`가 있고 여기서 덮어쓴 행은 없다는 차이로 구분한다. 그래야 오프셋
/// 로직이나 신뢰도 기준을 바꿨을 때 재실행만으로 다시 계산된다.
fn reconcile_inputs(
    rows: &[paintrobot_repo_coredb::JobRow],
) -> (Vec<domain::PlcState>, Vec<domain::CamEvent>) {
    let mut plc_events: Vec<domain::PlcState> = rows
        .iter()
        .filter(|r| r.match_status == "plc_only")
        .filter_map(|r| {
            Some(domain::PlcState {
                ts_ms: r.plc_ts.filter(|t| *t > 0)?,
                model_no: r.plc_model_no.clone().filter(|m| !m.is_empty())?,
            })
        })
        .collect();
    plc_events.sort_by_key(|p| p.ts_ms);
    let timeline = domain::plc_timeline(&plc_events);

    let mut cams: Vec<domain::CamEvent> = rows
        .iter()
        .filter(|r| {
            matches!(r.match_status.as_str(), "camera_only" | "matched" | "mismatch")
                && r.plc_ts.unwrap_or(0) == 0
        })
        .filter_map(|r| {
            Some(domain::CamEvent {
                event_id: r.event_id.clone(),
                ts_ms: r.camera_ts.filter(|t| *t > 0)?,
                model_no: r.camera_model_no.clone().filter(|m| !m.is_empty())?,
                confidence: r.confidence.unwrap_or(0.0),
            })
        })
        .collect();
    cams.sort_by_key(|c| c.ts_ms);

    (timeline, cams)
}

fn mixflow_json(date: &str, s: &domain::MixFlowStats) -> serde_json::Value {
    serde_json::json!({
        "work_date": date,
        "units": s.units,
        "models": s.models,
        "changeovers": s.changeovers,
        "changeover_rate": s.changeover_rate,
        "avg_run": s.avg_run,
        "max_run": s.max_run,
        "singles": s.singles,
        "runs": s.runs.iter().map(|r| serde_json::json!({
            "model_no": r.model_no,
            "count": r.count,
            "start_ms": r.start_ms,
        })).collect::<Vec<_>>(),
    })
}

fn reconcile_json(
    date: &str,
    rep: &domain::ReconcileReport,
    min_confidence: f64,
    dry_run: bool,
    written: u32,
    write_errors: u32,
) -> serde_json::Value {
    serde_json::json!({
        "work_date": date,
        "dry_run": dry_run,
        "min_confidence": min_confidence,
        "offset_secs": rep.offset_secs,
        "plc_states": rep.plc_states,
        "camera_events": rep.camera_events,
        "matched": rep.matched,
        "mismatch": rep.mismatch,
        "skipped_low_confidence": rep.skipped_low_confidence,
        "skipped_no_plc": rep.skipped_no_plc,
        "after_changeover": {
            "first_unit": bucket_json(&rep.first_unit),
            "early_units": bucket_json(&rep.early_units),
            "steady_units": bucket_json(&rep.steady_units),
        },
        "written": written,
        "write_errors": write_errors,
    })
}

fn bucket_json(b: &domain::MatchBucket) -> serde_json::Value {
    serde_json::json!({
        "matched": b.matched,
        "mismatch": b.mismatch,
        "total": b.total(),
        // 표본이 없으면 null. 0으로 내려보내면 화면에서 "이상 없음"으로 읽힌다.
        "mismatch_rate": b.mismatch_rate(),
    })
}

/// 하루치 집계를 미리 계산해 `daily_rollup`에 한 행으로 넣는다.
///
/// `jobs`의 PK가 `event_id`라 행 하나가 파티션 하나고, `WHERE work_date=...`
/// 스캔은 수십만 번의 개별 파티션 읽기가 된다 (실측 282초). 대시보드가 읽는
/// 값은 전부 일별 집계라, 그 비싼 스캔을 요청마다가 아니라 주기적으로 한 번만
/// 치르고 결과를 날짜당 한 행에 담아둔다.
///
/// 오늘치는 계속 바뀌므로 타이머가 주기적으로 다시 부른다.
async fn build_rollup(query: &str, headers: &wstd::http::HeaderMap) -> Response<Body> {
    if !check_edge_key(headers) {
        return json_error(StatusCode::UNAUTHORIZED, "missing or invalid X-Edge-Key");
    }
    let Some(date) = query_param(query, "date") else {
        return json_error(StatusCode::BAD_REQUEST, "missing date");
    };
    if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        return json_error(StatusCode::BAD_REQUEST, "date must be YYYY-MM-DD");
    }

    let c = client();
    // 하루에 딱 한 번만 스캔한다. 아래 세 집계가 전부 이 행들에서 나온다.
    let rows = match c.scan_jobs_for_date(&date, 1_000_000).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };

    let payload = rollup_payload(&date, &rows);
    let updated_at = Utc::now().timestamp_millis();
    let body = match serde_json::to_string(&payload) {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    match c
        .upsert_rollup(&paintrobot_repo_coredb::RollupRow {
            work_date: date.clone(),
            payload: body,
            updated_at,
        })
        .await
    {
        Ok(()) => json_response(
            StatusCode::OK,
            &serde_json::json!({
                "work_date": date,
                "rows_scanned": rows.len(),
                "updated_at": updated_at,
            }),
        ),
        Err(e) => repo_error_response(&e),
    }
}

/// 하루치 행에서 대시보드가 쓰는 집계를 전부 뽑는다.
///
/// 셋을 한 행에 같이 담는 이유: 전부 같은 스캔 결과에서 나오므로 따로 계산할
/// 이유가 없고, 대시보드도 한 번의 조회로 끝난다.
fn rollup_payload(date: &str, rows: &[paintrobot_repo_coredb::JobRow]) -> serde_json::Value {
    let agg: Vec<domain::AggRow> = rows
        .iter()
        .map(|r| domain::AggRow {
            model_no: r
                .plc_model_no
                .clone()
                .filter(|m| !m.is_empty())
                .or_else(|| r.camera_model_no.clone().filter(|m| !m.is_empty()))
                .unwrap_or_else(|| "(unknown)".to_string()),
            match_status: r.match_status.clone(),
        })
        .collect();
    let stats = domain::aggregate(date.to_string(), agg);

    let mut seq: Vec<(i64, String)> = rows
        .iter()
        .filter(|r| r.match_status != "plc_only")
        .filter_map(|r| {
            let ts = r.camera_ts.filter(|t| *t > 0).or(r.plc_ts.filter(|t| *t > 0))?;
            let model = r
                .camera_model_no
                .clone()
                .filter(|m| !m.is_empty())
                .or_else(|| r.plc_model_no.clone().filter(|m| !m.is_empty()))?;
            Some((ts, model))
        })
        .collect();
    seq.sort_by_key(|(ts, _)| *ts);
    let mix = domain::mix_flow(&seq);

    let (timeline, cams) = reconcile_inputs(rows);
    let (_, rep) = domain::reconcile(&timeline, &cams, DEFAULT_MIN_CONFIDENCE);

    serde_json::json!({
        "stats": stats,
        "mixflow": mixflow_json(date, &mix),
        "reconcile": reconcile_json(date, &rep, DEFAULT_MIN_CONFIDENCE, true, 0, 0),
    })
}

/// 혼류 생산 지표 — 순서에서만 나오는 값들.
///
/// 일자별 합계로는 혼류가 보이지 않는다. 같은 100대라도 한 차종을 몰아서
/// 만든 것과 여러 차종이 섞여 흐른 것은 라인에 전혀 다른 부담인데, 합계를
/// 내는 순간 그 차이가 사라진다.
async fn stats_mixflow(query: &str) -> Response<Body> {
    let Some(date) = query_param(query, "date") else {
        return json_error(StatusCode::BAD_REQUEST, "missing date");
    };
    if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        return json_error(StatusCode::BAD_REQUEST, "date must be YYYY-MM-DD");
    }

    if let Some(v) = rollup_part(&date, "mixflow").await {
        return json_response(StatusCode::OK, &v);
    }

    let c = client();
    let rows = match c.scan_jobs_for_date(&date, 1_000_000).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };

    // 생산으로 세는 기준은 `domain::aggregate`와 같다 — plc_only는 PLC 상태
    // 갱신이지 차가 아니다. 그래야 화면의 생산 대수와 어긋나지 않는다.
    let mut seq: Vec<(i64, String)> = rows
        .iter()
        .filter(|r| r.match_status != "plc_only")
        .filter_map(|r| {
            let ts = r.camera_ts.filter(|t| *t > 0).or(r.plc_ts.filter(|t| *t > 0))?;
            let model = r
                .camera_model_no
                .clone()
                .filter(|m| !m.is_empty())
                .or_else(|| r.plc_model_no.clone().filter(|m| !m.is_empty()))?;
            Some((ts, model))
        })
        .collect();
    seq.sort_by_key(|(ts, _)| *ts);

    let stats = domain::mix_flow(&seq);
    let runs: Vec<_> = stats
        .runs
        .iter()
        .map(|r| {
            serde_json::json!({
                "model_no": r.model_no,
                "count": r.count,
                "start_ms": r.start_ms,
            })
        })
        .collect();

    json_response(
        StatusCode::OK,
        &serde_json::json!({
            "work_date": date,
            "units": stats.units,
            "models": stats.models,
            "changeovers": stats.changeovers,
            "changeover_rate": stats.changeover_rate,
            "avg_run": stats.avg_run,
            "max_run": stats.max_run,
            "singles": stats.singles,
            "runs": runs,
        }),
    )
}

/// PLC 상태와 카메라 인식을 사후에 이어붙여 `match_status`를 다시 매긴다.
///
/// 왜 사후인가: 카메라가 PLC보다 앞선다. 투입구에서 읽힌 차가 도장 부스에
/// 도착해야 PLC 상태에 반영되므로, 카메라 이벤트가 들어오는 순간에는 짝이 될
/// PLC 지시가 아직 없다. 그래서 ingest 시점에는 못 하고 하루가 지난 뒤 돌린다.
///
/// `dry_run=true`(기본)면 무엇이 바뀔지만 계산하고 쓰지 않는다. 실제로 쓰려면
/// `dry_run=false`를 명시해야 한다 — 판정 결과를 덮어쓰는 일이라 기본값을
/// 안전한 쪽에 둔다.
async fn reconcile_jobs(
    query: &str,
    headers: Option<&wstd::http::HeaderMap>,
) -> Response<Body> {
    // headers가 없으면 GET(관찰용) — 절대 쓰지 않는다.
    let may_write = match headers {
        Some(h) => {
            if !check_edge_key(h) {
                return json_error(StatusCode::UNAUTHORIZED, "missing or invalid X-Edge-Key");
            }
            true
        }
        None => false,
    };
    let Some(date) = query_param(query, "date") else {
        return json_error(StatusCode::BAD_REQUEST, "missing date");
    };
    if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        return json_error(StatusCode::BAD_REQUEST, "date must be YYYY-MM-DD");
    }
    let dry_run = !may_write || query_param(query, "dry_run").as_deref() != Some("false");
    let min_confidence = query_param(query, "min_confidence")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(DEFAULT_MIN_CONFIDENCE);

    // 관찰용 조회는 미리 계산된 값으로 끝낸다. 쓰기 요청은 최신 상태가
    // 필요하므로 항상 다시 계산한다.
    if !may_write {
        if let Some(v) = rollup_part(&date, "reconcile").await {
            return json_response(StatusCode::OK, &v);
        }
    }

    let c = client();
    let rows = match c.scan_jobs_for_date(&date, 1_000_000).await {
        Ok(r) => r,
        Err(e) => return repo_error_response(&e),
    };

    let (timeline, cams) = reconcile_inputs(&rows);

    let (decisions, report) = domain::reconcile(&timeline, &cams, min_confidence);

    let mut written = 0u32;
    let mut write_errors = 0u32;
    if !dry_run {
        for d in &decisions {
            let Some(row) = rows.iter().find(|r| r.event_id == d.event_id) else {
                continue;
            };
            match c.rewrite_job_match(row, &d.plc_model_no, d.status).await {
                Ok(()) => written += 1,
                Err(_) => write_errors += 1,
            }
        }
    }

    json_response(
        StatusCode::OK,
        &serde_json::json!({
            "work_date": date,
            "dry_run": dry_run,
            "min_confidence": min_confidence,
            "offset_secs": report.offset_secs,
            "plc_states": report.plc_states,
            "camera_events": report.camera_events,
            "matched": report.matched,
            "mismatch": report.mismatch,
            "skipped_low_confidence": report.skipped_low_confidence,
            "skipped_no_plc": report.skipped_no_plc,
            // 전환 직후 구간별. 구간은 카메라 쪽 런 위치로 나눈 값이라
            // 추정 오프셋의 오차가 섞이지 않는다.
            "after_changeover": {
                "first_unit": bucket_json(&report.first_unit),
                "early_units": bucket_json(&report.early_units),
                "steady_units": bucket_json(&report.steady_units),
            },
            "written": written,
            "write_errors": write_errors,
        }),
    )
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

/// 이 아래 신뢰도의 카메라 판독은 정합 판정에서 제외한다. 카메라를 못 믿는
/// 상황에서 "불일치"라고 적으면 없는 품질 이상을 만들어내는 셈이다.
const DEFAULT_MIN_CONFIDENCE: f64 = 0.7;

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
