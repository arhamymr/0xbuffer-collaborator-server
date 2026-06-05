mod api;
mod config;
mod database;
mod dns;
mod listeners;
mod models;
mod payloads;
mod state;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::Router;
use tokio::task::JoinSet;
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing::{error, info};

use crate::{config::AppConfig, database::Database, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "apprecon_collaborator=info,tower_http=info".into()),
        )
        .init();

    let config = AppConfig::load().context("failed to load configuration")?;
    let database = Database::connect(&config.database)
        .await
        .context("failed to connect database")?;
    database
        .migrate()
        .await
        .context("failed to migrate database")?;

    let state = Arc::new(AppState::new(config.clone(), database));
    let api = api::router(state.clone())
        .layer(RequestBodyLimitLayer::new(config.security.max_body_bytes))
        .layer(TraceLayer::new_for_http());
    let callback = listeners::http_router(state.clone(), "http")
        .layer(RequestBodyLimitLayer::new(config.security.max_body_bytes))
        .layer(TraceLayer::new_for_http());

    let api_addr = config.server.socket_addr()?;
    let http_addr = config.http.socket_addr()?;
    let dns_udp_addr = config.dns.udp_socket_addr()?;
    let dns_tcp_addr = config.dns.tcp_socket_addr()?;

    let mut tasks = JoinSet::new();
    tasks.spawn(serve_axum("api", api_addr, api));
    tasks.spawn(serve_axum("http-listener", http_addr, callback.clone()));
    tasks.spawn(dns::serve_udp(dns_udp_addr, state.clone()));
    tasks.spawn(dns::serve_tcp(dns_tcp_addr, state.clone()));

    if config.tls.enabled {
        if let Some(https_addr) = config.tls.socket_addr()? {
            let tls_state = state.clone();
            let tls_router = listeners::http_router(tls_state, "https")
                .layer(RequestBodyLimitLayer::new(config.security.max_body_bytes))
                .layer(TraceLayer::new_for_http());
            let cert = config.tls.cert.clone();
            let key = config.tls.key.clone();
            tasks.spawn(async move {
                let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key)
                    .await
                    .context("failed to load TLS certificate/key")?;
                info!(%https_addr, "starting https-listener");
                axum_server::bind_rustls(https_addr, tls_config)
                    .serve(tls_router.into_make_service_with_connect_info::<SocketAddr>())
                    .await
                    .context("https-listener stopped")
            });
        }
    }

    tokio::select! {
        _ = shutdown_signal() => {
            info!("shutdown signal received");
            tasks.abort_all();
        }
        result = wait_for_failure(&mut tasks) => {
            result?;
        }
    }

    Ok(())
}

async fn serve_axum(name: &'static str, addr: SocketAddr, app: Router) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {name} at {addr}"))?;
    info!(%addr, "starting {name}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .with_context(|| format!("{name} stopped"))
}

async fn wait_for_failure(tasks: &mut JoinSet<anyhow::Result<()>>) -> anyhow::Result<()> {
    while let Some(task) = tasks.join_next().await {
        match task {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                error!(?err, "service task failed");
                tasks.abort_all();
                return Err(err);
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install terminate handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
