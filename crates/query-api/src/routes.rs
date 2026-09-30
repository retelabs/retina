use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use clickhouse::Client;
use kernel_model::TraceId;
use serde::{Deserialize, Serialize};

use crate::dto::{MetricsSummaryDto, SpanDto, TraceSummaryDto};
use crate::queries;

pub enum ApiError {
    Database(clickhouse::error::Error),
    InvalidTraceId(String),
    TraceNotFound(String),
}

impl From<clickhouse::error::Error> for ApiError {
    fn from(e: clickhouse::error::Error) -> Self {
        ApiError::Database(e)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::InvalidTraceId(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::TraceNotFound(trace_id) => (
                StatusCode::NOT_FOUND,
                format!("no spans found for trace_id {trace_id}"),
            ),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[derive(Deserialize)]
pub struct ListTracesParams {
    limit: Option<u64>,
}

/// `GET /traces?limit=N` — "list recent traces" (dossier step 4).
pub async fn list_traces(
    State(client): State<Client>,
    Query(params): Query<ListTracesParams>,
) -> Result<Json<Vec<TraceSummaryDto>>, ApiError> {
    // Cap the limit even though there's no retention policy yet (dossier
    // section 4 explicitly defers that) — an unbounded `?limit=` is still a
    // self-inflicted footgun, not something worth waiting on a retention ADR
    // to guard against.
    let limit = params.limit.unwrap_or(50).min(500);
    let traces = queries::list_recent_traces(&client, limit).await?;
    Ok(Json(traces))
}

/// `GET /traces/{trace_id}` — "fetch a trace's tree" (dossier step
/// 4), `trace_id` as the same lower-hex 32-char encoding OTLP/JSON uses.
pub async fn get_trace(
    State(client): State<Client>,
    Path(trace_id_hex): Path<String>,
) -> Result<Json<Vec<SpanDto>>, ApiError> {
    let bytes = hex::decode(&trace_id_hex)
        .map_err(|e| ApiError::InvalidTraceId(format!("trace_id must be 32 hex chars: {e}")))?;
    TraceId::try_from(bytes.as_slice()).map_err(|e| ApiError::InvalidTraceId(e.to_string()))?;

    let rows = queries::get_trace_spans(&client, &trace_id_hex).await?;
    if rows.is_empty() {
        return Err(ApiError::TraceNotFound(trace_id_hex));
    }
    Ok(Json(rows.into_iter().map(SpanDto::from).collect()))
}

/// `GET /metrics/summary` — "aggregate a few basic metrics" (dossier
/// step 4).
pub async fn metrics_summary(
    State(client): State<Client>,
) -> Result<Json<MetricsSummaryDto>, ApiError> {
    Ok(Json(queries::metrics_summary(&client).await?))
}
