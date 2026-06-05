use std::fs;

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

use crate::{
    config::DatabaseConfig,
    models::{CreatePayloadRequest, Interaction, NewInteraction, Payload, StatsResponse},
    payloads,
};

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn connect(config: &DatabaseConfig) -> anyhow::Result<Self> {
        if config.driver != "sqlite" {
            bail!("unsupported database driver: {}", config.driver);
        }

        if let Some(parent) = config.path.parent() {
            fs::create_dir_all(parent).context("failed to create database directory")?;
        }

        let url = format!("sqlite://{}?mode=rwc", config.path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS payloads (
                id TEXT PRIMARY KEY,
                payload_id TEXT UNIQUE NOT NULL,
                name TEXT,
                description TEXT,
                tags TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                expires_at TEXT
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS interactions (
                id TEXT PRIMARY KEY,
                payload_id TEXT NOT NULL,
                interaction_type TEXT NOT NULL,
                source_ip TEXT,
                protocol TEXT NOT NULL,
                method TEXT,
                path TEXT,
                query_type TEXT,
                headers TEXT NOT NULL DEFAULT '{}',
                body TEXT,
                tls_metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_interactions_payload_id ON interactions(payload_id);",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_interactions_created_at ON interactions(created_at);",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_payload(
        &self,
        root_domain: &str,
        request: CreatePayloadRequest,
    ) -> anyhow::Result<Payload> {
        let id = Uuid::new_v4();
        let payload_id = payloads::generate_payload_id();
        let created_at = Utc::now();
        let tags = if request.tags.is_null() {
            json!({})
        } else {
            request.tags
        };

        sqlx::query(
            r#"
            INSERT INTO payloads
                (id, payload_id, name, description, tags, created_at, expires_at)
            VALUES
                (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&payload_id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(tags.to_string())
        .bind(created_at.to_rfc3339())
        .bind(request.expires_at.map(|date| date.to_rfc3339()))
        .execute(&self.pool)
        .await?;

        Ok(Payload {
            id,
            payload: payloads::fqdn(&payload_id, root_domain),
            payload_id,
            name: request.name,
            description: request.description,
            tags,
            created_at,
            expires_at: request.expires_at,
        })
    }

    pub async fn list_payloads(&self, root_domain: &str) -> anyhow::Result<Vec<Payload>> {
        let rows = sqlx::query(
            r#"
            SELECT id, payload_id, name, description, tags, created_at, expires_at
            FROM payloads
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| payload_from_row(row, root_domain))
            .collect()
    }

    pub async fn payload_exists_and_active(&self, payload_id: &str) -> anyhow::Result<bool> {
        let row = sqlx::query("SELECT expires_at FROM payloads WHERE payload_id = ?")
            .bind(payload_id)
            .fetch_optional(&self.pool)
            .await?;

        let Some(row) = row else {
            return Ok(false);
        };

        let expires_at: Option<String> = row.try_get("expires_at")?;
        match expires_at {
            Some(expires_at) => Ok(parse_time(&expires_at)? > Utc::now()),
            None => Ok(true),
        }
    }

    pub async fn insert_interaction(&self, interaction: NewInteraction) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO interactions
                (id, payload_id, interaction_type, source_ip, protocol, method, path,
                 query_type, headers, body, tls_metadata, created_at)
            VALUES
                (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(interaction.payload_id)
        .bind(interaction.interaction_type)
        .bind(interaction.source_ip)
        .bind(interaction.protocol)
        .bind(interaction.method)
        .bind(interaction.path)
        .bind(interaction.query_type)
        .bind(interaction.headers.to_string())
        .bind(interaction.body)
        .bind(interaction.tls_metadata.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn list_interactions(
        &self,
        payload_id: Option<String>,
    ) -> anyhow::Result<Vec<Interaction>> {
        let rows = if let Some(payload_id) = payload_id {
            sqlx::query(
                r#"
                SELECT id, payload_id, interaction_type, source_ip, protocol, method, path,
                       query_type, headers, body, tls_metadata, created_at
                FROM interactions
                WHERE payload_id = ?
                ORDER BY created_at DESC
                LIMIT 500
                "#,
            )
            .bind(payload_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, payload_id, interaction_type, source_ip, protocol, method, path,
                       query_type, headers, body, tls_metadata, created_at
                FROM interactions
                ORDER BY created_at DESC
                LIMIT 500
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };

        rows.into_iter().map(interaction_from_row).collect()
    }

    pub async fn interaction(&self, id: Uuid) -> anyhow::Result<Option<Interaction>> {
        let row = sqlx::query(
            r#"
            SELECT id, payload_id, interaction_type, source_ip, protocol, method, path,
                   query_type, headers, body, tls_metadata, created_at
            FROM interactions
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(interaction_from_row).transpose()
    }

    pub async fn stats(&self) -> anyhow::Result<StatsResponse> {
        let payload_count = count(&self.pool, "payloads").await?;
        let interaction_count = count(&self.pool, "interactions").await?;
        let since = (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();

        let requests_per_minute: i64 = sqlx::query(
            "SELECT COUNT(*) AS count FROM interactions WHERE protocol IN ('http', 'https') AND created_at >= ?",
        )
        .bind(&since)
        .fetch_one(&self.pool)
        .await?
        .try_get("count")?;

        let dns_queries_per_minute: i64 = sqlx::query(
            "SELECT COUNT(*) AS count FROM interactions WHERE protocol = 'dns' AND created_at >= ?",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await?
        .try_get("count")?;

        Ok(StatsResponse {
            payload_count,
            interaction_count,
            requests_per_minute,
            dns_queries_per_minute,
        })
    }
}

async fn count(pool: &SqlitePool, table: &str) -> anyhow::Result<i64> {
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    Ok(sqlx::query(&sql).fetch_one(pool).await?.try_get("count")?)
}

fn payload_from_row(row: sqlx::sqlite::SqliteRow, root_domain: &str) -> anyhow::Result<Payload> {
    let payload_id: String = row.try_get("payload_id")?;
    Ok(Payload {
        id: parse_uuid(row.try_get::<String, _>("id")?)?,
        payload: payloads::fqdn(&payload_id, root_domain),
        payload_id,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        tags: parse_json(row.try_get::<String, _>("tags")?)?,
        created_at: parse_time(&row.try_get::<String, _>("created_at")?)?,
        expires_at: row
            .try_get::<Option<String>, _>("expires_at")?
            .map(|value| parse_time(&value))
            .transpose()?,
    })
}

fn interaction_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Interaction> {
    Ok(Interaction {
        id: parse_uuid(row.try_get::<String, _>("id")?)?,
        payload_id: row.try_get("payload_id")?,
        interaction_type: row.try_get("interaction_type")?,
        source_ip: row.try_get("source_ip")?,
        protocol: row.try_get("protocol")?,
        method: row.try_get("method")?,
        path: row.try_get("path")?,
        query_type: row.try_get("query_type")?,
        headers: parse_json(row.try_get::<String, _>("headers")?)?,
        body: row.try_get("body")?,
        tls_metadata: parse_json(row.try_get::<String, _>("tls_metadata")?)?,
        created_at: parse_time(&row.try_get::<String, _>("created_at")?)?,
    })
}

fn parse_uuid(value: String) -> anyhow::Result<Uuid> {
    Uuid::parse_str(&value).context("invalid UUID in database")
}

fn parse_time(value: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn parse_json(value: String) -> anyhow::Result<Value> {
    Ok(serde_json::from_str(&value)?)
}
