use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    models::{CreatePayloadRequest, CreatePayloadResponse, HealthResponse},
    state::AppState,
};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/v1/payloads", post(create_payload).get(list_payloads))
        .route("/api/v1/interactions", get(list_interactions))
        .route("/api/v1/interactions/:id", get(interaction_detail))
        .route("/api/v1/statistics", get(statistics))
        .with_state(state)
}

async fn create_payload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CreatePayloadRequest>,
) -> Result<Json<CreatePayloadResponse>, ApiError> {
    authorize(&state, &headers)?;
    let payload = state
        .database
        .create_payload(&state.config.domain.root, request)
        .await?;
    Ok(Json(CreatePayloadResponse {
        id: payload.id,
        payload_id: payload.payload_id,
        payload: payload.payload,
        expires_at: payload.expires_at,
    }))
}

async fn list_payloads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    authorize(&state, &headers)?;
    Ok(Json(
        state
            .database
            .list_payloads(&state.config.domain.root)
            .await?,
    )
    .into_response())
}

async fn list_interactions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    authorize(&state, &headers)?;
    let payload_id = params.get("payload_id").cloned();
    Ok(Json(state.database.list_interactions(payload_id).await?).into_response())
}

async fn interaction_detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    authorize(&state, &headers)?;
    match state.database.interaction(id).await? {
        Some(interaction) => Ok(Json(interaction).into_response()),
        None => Err(ApiError::not_found("interaction not found")),
    }
}

async fn health(State(state): State<Arc<AppState>>) -> Result<Json<HealthResponse>, ApiError> {
    let stats = state.database.stats().await?;
    Ok(Json(HealthResponse {
        status: "ok",
        payload_count: stats.payload_count,
        interaction_count: stats.interaction_count,
    }))
}

async fn statistics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    authorize(&state, &headers)?;
    Ok(Json(state.database.stats().await?).into_response())
}

async fn metrics(State(state): State<Arc<AppState>>) -> Result<Response, ApiError> {
    let stats = state.database.stats().await?;
    let body = format!(
        "# TYPE hexbuffer_payload_count gauge\nhexbuffer_payload_count {}\n\
         # TYPE hexbuffer_interaction_count gauge\nhexbuffer_interaction_count {}\n\
         # TYPE hexbuffer_requests_per_minute gauge\nhexbuffer_requests_per_minute {}\n\
         # TYPE hexbuffer_dns_queries_per_minute gauge\nhexbuffer_dns_queries_per_minute {}\n",
        stats.payload_count,
        stats.interaction_count,
        stats.requests_per_minute,
        stats.dns_queries_per_minute
    );
    Ok((
        [(http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    )
        .into_response())
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = format!("Bearer {}", state.config.security.api_key);
    let Some(value) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(ApiError::unauthorized());
    };

    if value == expected {
        Ok(())
    } else {
        Err(ApiError::unauthorized())
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "unauthorized".to_string(),
        }
    }

    fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}
