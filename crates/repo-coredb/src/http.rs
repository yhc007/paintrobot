//! Pluggable HTTP transport for CoreDbClient.
//!
//! Native builds (tests, CLI tools) get a reqwest-backed transport.
//! The WASM api-gateway will wire a wasi:http transport by implementing
//! `HttpTransport` on its own type.

use async_trait::async_trait;

#[derive(Debug)]
pub struct TransportError(pub String);

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TransportError {}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn post_json(&self, url: &str, body: String) -> Result<String, TransportError>;
}

#[cfg(not(target_family = "wasm"))]
pub use native::ReqwestTransport;
#[cfg(target_family = "wasm")]
pub use wasi::WasiTransport;

#[cfg(not(target_family = "wasm"))]
mod native {
    use super::{async_trait, HttpTransport, TransportError};
    use std::time::Duration;

    pub struct ReqwestTransport {
        client: reqwest::Client,
    }

    impl ReqwestTransport {
        pub fn new() -> Self {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client");
            Self { client }
        }
    }

    impl Default for ReqwestTransport {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl HttpTransport for ReqwestTransport {
        async fn post_json(&self, url: &str, body: String) -> Result<String, TransportError> {
            let resp = self
                .client
                .post(url)
                .header("content-type", "application/json")
                .body(body)
                .send()
                .await
                .map_err(|e| TransportError(e.to_string()))?;
            let text = resp.text().await.map_err(|e| TransportError(e.to_string()))?;
            Ok(text)
        }
    }
}

#[cfg(target_family = "wasm")]
mod wasi {
    use super::{async_trait, HttpTransport, TransportError};
    use http_body_util::BodyExt;
    use wstd::http::{Body, Client, Request};

    pub struct WasiTransport;

    impl WasiTransport {
        pub fn new() -> Self {
            Self
        }
    }

    impl Default for WasiTransport {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl HttpTransport for WasiTransport {
        async fn post_json(&self, url: &str, body: String) -> Result<String, TransportError> {
            let req = Request::builder()
                .method("POST")
                .uri(url)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .map_err(|e| TransportError(format!("build request: {e}")))?;
            let resp = Client::new()
                .send(req)
                .await
                .map_err(|e| TransportError(format!("send: {e}")))?;
            let collected = resp
                .into_body()
                .into_boxed_body()
                .collect()
                .await
                .map_err(|e| TransportError(format!("read body: {e}")))?;
            let bytes = collected.to_bytes();
            String::from_utf8(bytes.to_vec())
                .map_err(|e| TransportError(format!("body utf8: {e}")))
        }
    }
}
