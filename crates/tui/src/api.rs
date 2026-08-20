//! Thin HTTP client for `query-api` — reuses its exact DTOs
//! (`query_api::dto`) rather than a second hand-written copy of the wire
//! shape. Same auth convention as every other Venice surface
//! (docs/interfaces/kernel-auth.md): `authorization: Bearer <QUERY_API_KEY>`.

use query_api::dto::{MetricsSummaryDto, SpanDto, TraceSummaryDto};
use reqwest::Client;

pub struct ApiClient {
    http: Client,
    base_url: String,
    api_key: String,
}

#[derive(Debug)]
pub enum ApiError {
    Request(reqwest::Error),
    /// Non-2xx response — carries the status so the UI can distinguish
    /// "wrong token" (401) from a real server error, not just show one
    /// generic failure message for both.
    Status(reqwest::StatusCode, String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Request(e) => write!(f, "request failed: {e}"),
            ApiError::Status(status, body) => write!(f, "{status}: {body}"),
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Request(e)
    }
}

impl ApiClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            http: Client::new(),
            base_url,
            api_key,
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        Ok(response.json::<T>().await?)
    }

    pub async fn list_traces(&self, limit: u64) -> Result<Vec<TraceSummaryDto>, ApiError> {
        self.get_json(&format!("/traces?limit={limit}")).await
    }

    pub async fn get_trace(&self, trace_id: &str) -> Result<Vec<SpanDto>, ApiError> {
        self.get_json(&format!("/traces/{trace_id}")).await
    }

    pub async fn metrics_summary(&self) -> Result<MetricsSummaryDto, ApiError> {
        self.get_json("/metrics/summary").await
    }
}
