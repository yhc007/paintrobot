//! HTTP client for CoreDB's POST /query endpoint.
//!
//! CoreDB response envelope:
//!   write-ok : {"status":"success","message":"Query executed successfully"}
//!   read-ok  : {"status":"success","data":[{"columns":{col: {Type: value}, ...}}]}
//!   error    : {"status":"error","message":"..."}
//!
//! Column values are tagged with their type:
//!   {"Text": "..."} | {"Int": 1} | {"BigInt": 12345} | {"Double": 1.2} |
//!   {"Boolean": true} | {"UUID": "..."} | "Null"
//!
//! CoreDB has no parameter binding and its parser is whitespace-fragile, so
//! the client serializes values directly into the CQL string via the helpers
//! in `cql`. Identifier inputs (model_no, edge_id, event_id) must pass
//! `check_identifier` before embedding.

mod coatings;
mod cql;
mod http;
mod jobs;
mod recipes;
mod weather;

pub use coatings::CoatingRow;
pub use cql::{check_identifier, fmt_double, quote_text};
pub use http::{HttpTransport, TransportError};
#[cfg(not(target_family = "wasm"))]
pub use http::ReqwestTransport;
#[cfg(target_family = "wasm")]
pub use http::WasiTransport;
pub use jobs::JobRow;
pub use recipes::RecipeRow;
pub use weather::WeatherRow;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("coredb error: {0}")]
    Db(String),
    #[error("transport: {0}")]
    Transport(String),
    #[error("decode: {0}")]
    Decode(String),
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),
}

impl From<TransportError> for RepoError {
    fn from(e: TransportError) -> Self {
        RepoError::Transport(e.0)
    }
}

#[derive(Debug, Deserialize)]
struct Envelope {
    status: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<Vec<RawRow>>,
}

#[derive(Debug, Deserialize)]
pub struct RawRow {
    pub columns: serde_json::Map<String, serde_json::Value>,
}

pub struct CoreDbClient<T: HttpTransport> {
    transport: T,
    base_url: String,
    pub keyspace: String,
}

impl<T: HttpTransport> CoreDbClient<T> {
    pub fn new(transport: T, base_url: impl Into<String>, keyspace: impl Into<String>) -> Self {
        Self {
            transport,
            base_url: base_url.into(),
            keyspace: keyspace.into(),
        }
    }

    /// Execute a CQL statement. Returns rows for SELECT, empty Vec otherwise.
    pub async fn execute(&self, cql: &str) -> Result<Vec<RawRow>, RepoError> {
        let url = format!("{}/query", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({ "query": cql }).to_string();
        let resp = self.transport.post_json(&url, body).await?;
        let env: Envelope = serde_json::from_str(&resp)
            .map_err(|e| RepoError::Decode(format!("{}: body={}", e, resp)))?;
        if env.status != "success" {
            return Err(RepoError::Db(env.message.unwrap_or_default()));
        }
        Ok(env.data.unwrap_or_default())
    }
}

/// Pull a String out of a CoreDB tagged value. Accepts {"Text":"..."}.
pub(crate) fn decode_text(v: &serde_json::Value) -> Result<String, RepoError> {
    v.get("Text")
        .and_then(|t| t.as_str())
        .map(str::to_owned)
        .ok_or_else(|| RepoError::Decode(format!("expected Text, got {v}")))
}

/// Pull a String from a tagged Text, returning None if the value is "Null".
pub(crate) fn decode_text_opt(v: &serde_json::Value) -> Result<Option<String>, RepoError> {
    if v.as_str() == Some("Null") {
        return Ok(None);
    }
    decode_text(v).map(Some)
}

/// Pull an i64 out of a CoreDB tagged value. Accepts {"BigInt":N} or {"Int":N}.
pub(crate) fn decode_i64(v: &serde_json::Value) -> Result<i64, RepoError> {
    if let Some(n) = v.get("BigInt").and_then(|t| t.as_i64()) {
        return Ok(n);
    }
    if let Some(n) = v.get("Int").and_then(|t| t.as_i64()) {
        return Ok(n);
    }
    Err(RepoError::Decode(format!("expected BigInt/Int, got {v}")))
}

/// Pull an f64 out of a CoreDB tagged value. Accepts {"Double":N}.
pub(crate) fn decode_f64(v: &serde_json::Value) -> Result<f64, RepoError> {
    v.get("Double")
        .and_then(|t| t.as_f64())
        .ok_or_else(|| RepoError::Decode(format!("expected Double, got {v}")))
}

pub(crate) fn decode_f64_opt(v: &serde_json::Value) -> Result<Option<f64>, RepoError> {
    if v.as_str() == Some("Null") {
        return Ok(None);
    }
    decode_f64(v).map(Some)
}
