//! Static site info and runtime config.

use chrono::FixedOffset;

pub const SITE_NAME: &str = "현대정밀";
pub const SITE_ADDR: &str = "경남 창원시 의창구 반계로 3";
pub const SITE_LAT: f64 = 35.2706;
pub const SITE_LON: f64 = 128.6311;

pub fn kst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("KST offset")
}

/// Read the API gateway listen address.
pub fn addr() -> String {
    std::env::var("ADDR").unwrap_or_else(|_| "0.0.0.0:18080".to_string())
}

/// Read the CoreDB HTTP URL.
pub fn coredb_url() -> String {
    std::env::var("COREDB_URL").unwrap_or_else(|_| "http://127.0.0.1:9043".to_string())
}

pub fn coredb_keyspace() -> String {
    std::env::var("COREDB_KEYSPACE").unwrap_or_else(|_| "paintrobot".to_string())
}

pub fn owm_api_key() -> Option<String> {
    std::env::var("OWM_API_KEY").ok().filter(|s| !s.is_empty())
}

/// Parsed list of accepted edge API keys from `EDGE_API_KEYS` (comma separated).
/// Empty vec means auth is disabled (development mode).
pub fn edge_api_keys() -> Vec<String> {
    std::env::var("EDGE_API_KEYS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .collect()
        })
        .unwrap_or_default()
}
