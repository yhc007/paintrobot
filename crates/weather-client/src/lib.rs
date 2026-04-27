//! Weather provider abstraction with an OpenWeatherMap implementation.
//!
//! The transport layer is cfg-split so the same OwmProvider can run on
//! native (tests) and inside the wasm32-wasip2 api-gateway component.

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum WeatherError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("decode: {0}")]
    Decode(String),
    #[error("upstream status {0}: {1}")]
    Upstream(u16, String),
    #[error("missing API key")]
    MissingKey,
}

#[derive(Debug, Clone)]
pub struct Weather {
    pub observed_at: DateTime<FixedOffset>,
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub source: &'static str,
}

#[async_trait]
pub trait WeatherProvider: Send + Sync {
    async fn current(&self, lat: f64, lon: f64) -> Result<Weather, WeatherError>;
}

pub struct OwmProvider {
    pub api_key: String,
}

impl OwmProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self { api_key: api_key.into() }
    }
}

#[derive(Debug, Deserialize)]
struct OwmMain {
    temp: f64,
    humidity: f64,
}

#[derive(Debug, Deserialize)]
struct OwmResponse {
    main: OwmMain,
    dt: i64,
}

fn parse_owm(body: &str) -> Result<Weather, WeatherError> {
    let r: OwmResponse = serde_json::from_str(body)
        .map_err(|e| WeatherError::Decode(format!("{e}: body={body}")))?;
    let kst = FixedOffset::east_opt(9 * 3600).expect("KST");
    let observed_at = Utc
        .timestamp_opt(r.dt, 0)
        .single()
        .ok_or_else(|| WeatherError::Decode(format!("bad dt: {}", r.dt)))?
        .with_timezone(&kst);
    Ok(Weather {
        observed_at,
        temperature_c: r.main.temp,
        humidity_pct: r.main.humidity,
        source: "owm",
    })
}

fn owm_url(api_key: &str, lat: f64, lon: f64) -> String {
    format!(
        "https://api.openweathermap.org/data/2.5/weather?lat={lat}&lon={lon}&appid={key}&units=metric",
        key = api_key,
    )
}

// ── native (reqwest) ───────────────────────────────────────────────────────

#[cfg(not(target_family = "wasm"))]
#[async_trait]
impl WeatherProvider for OwmProvider {
    async fn current(&self, lat: f64, lon: f64) -> Result<Weather, WeatherError> {
        if self.api_key.is_empty() {
            return Err(WeatherError::MissingKey);
        }
        let url = owm_url(&self.api_key, lat, lon);
        let resp = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| WeatherError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| WeatherError::Transport(e.to_string()))?;
        if status >= 400 {
            return Err(WeatherError::Upstream(status, body));
        }
        parse_owm(&body)
    }
}

// ── wasm (wstd) ────────────────────────────────────────────────────────────

#[cfg(target_family = "wasm")]
#[async_trait]
impl WeatherProvider for OwmProvider {
    async fn current(&self, lat: f64, lon: f64) -> Result<Weather, WeatherError> {
        use http_body_util::BodyExt;
        use wstd::http::{Body, Client, Request};

        if self.api_key.is_empty() {
            return Err(WeatherError::MissingKey);
        }
        let url = owm_url(&self.api_key, lat, lon);
        let req = Request::builder()
            .method("GET")
            .uri(&url)
            .body(Body::empty())
            .map_err(|e| WeatherError::Transport(format!("build: {e}")))?;
        let resp = Client::new()
            .send(req)
            .await
            .map_err(|e| WeatherError::Transport(format!("send: {e}")))?;
        let status = resp.status().as_u16();
        let collected = resp
            .into_body()
            .into_boxed_body()
            .collect()
            .await
            .map_err(|e| WeatherError::Transport(format!("body: {e}")))?;
        let body = String::from_utf8(collected.to_bytes().to_vec())
            .map_err(|e| WeatherError::Decode(format!("utf8: {e}")))?;
        if status >= 400 {
            return Err(WeatherError::Upstream(status, body));
        }
        parse_owm(&body)
    }
}
