use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreatePayloadRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Value,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct CreatePayloadResponse {
    pub id: Uuid,
    pub payload_id: String,
    pub payload: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Payload {
    pub id: Uuid,
    pub payload_id: String,
    pub payload: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Interaction {
    pub id: Uuid,
    pub payload_id: String,
    pub interaction_type: String,
    pub source_ip: Option<String>,
    pub protocol: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub query_type: Option<String>,
    pub headers: Value,
    pub body: Option<String>,
    pub tls_metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct NewInteraction {
    pub payload_id: String,
    pub interaction_type: String,
    pub source_ip: Option<String>,
    pub protocol: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub query_type: Option<String>,
    pub headers: Value,
    pub body: Option<String>,
    pub tls_metadata: Value,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub payload_count: i64,
    pub interaction_count: i64,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub payload_count: i64,
    pub interaction_count: i64,
    pub requests_per_minute: i64,
    pub dns_queries_per_minute: i64,
}
