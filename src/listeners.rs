use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    body::Bytes,
    extract::{ConnectInfo, OriginalUri, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    routing::any,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Map, Value, json};
use tracing::warn;

use crate::{models::NewInteraction, payloads, state::AppState};

#[derive(Clone)]
struct CallbackState {
    app: Arc<AppState>,
    protocol: &'static str,
}

pub fn http_router(state: Arc<AppState>, protocol: &'static str) -> Router {
    Router::new()
        .route("/", any(capture_http))
        .route("/*path", any(capture_http))
        .with_state(CallbackState {
            app: state,
            protocol,
        })
}

async fn capture_http(
    State(state): State<CallbackState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let host = headers
        .get(http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|host| host.split(':').next())
        .unwrap_or_default();

    let Some(payload_id) = payloads::extract_payload_id(host, &state.app.config.domain.root) else {
        return (StatusCode::NOT_FOUND, "unknown collaborator payload").into_response();
    };

    match state
        .app
        .database
        .payload_exists_and_active(&payload_id)
        .await
    {
        Ok(true) => {}
        Ok(false) => return (StatusCode::GONE, "expired or unknown payload").into_response(),
        Err(error) => {
            warn!(?error, "failed to validate payload");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let body_text = String::from_utf8_lossy(&body).into_owned();
    let interaction = NewInteraction {
        payload_id,
        interaction_type: "http_request".to_string(),
        source_ip: Some(addr.ip().to_string()),
        protocol: state.protocol.to_string(),
        method: Some(method.to_string()),
        path: Some(uri.to_string()),
        query_type: None,
        headers: headers_to_json(&headers),
        body: Some(body_text),
        tls_metadata: json!({ "enabled": state.protocol == "https" }),
    };

    match state.app.database.insert_interaction(interaction).await {
        Ok(id) => (StatusCode::OK, format!("captured {id}\n")).into_response(),
        Err(error) => {
            warn!(?error, "failed to store HTTP interaction");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn headers_to_json(headers: &HeaderMap) -> Value {
    let mut map = Map::new();
    for (name, value) in headers {
        let entry = map
            .entry(name.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(values) = entry {
            values.push(Value::String(
                value
                    .to_str()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|_| BASE64.encode(value.as_bytes())),
            ));
        }
    }
    Value::Object(map)
}
