use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub server: BindConfig,
    pub http: BindConfig,
    pub dns: DnsConfig,
    pub domain: DomainConfig,
    pub database: DatabaseConfig,
    pub security: SecurityConfig,
    pub tls: TlsConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BindConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DnsConfig {
    pub host: String,
    pub udp_port: u16,
    pub tcp_port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DomainConfig {
    pub root: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    pub driver: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SecurityConfig {
    pub api_key: String,
    pub max_body_bytes: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub cert: PathBuf,
    pub key: PathBuf,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::Environment::with_prefix("APPRECON").separator("__"))
            .build()?;

        config.try_deserialize().context("invalid configuration")
    }
}

impl BindConfig {
    pub fn socket_addr(&self) -> anyhow::Result<SocketAddr> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .with_context(|| format!("invalid bind address {}:{}", self.host, self.port))
    }
}

impl DnsConfig {
    pub fn udp_socket_addr(&self) -> anyhow::Result<SocketAddr> {
        format!("{}:{}", self.host, self.udp_port)
            .parse()
            .context("invalid DNS UDP address")
    }

    pub fn tcp_socket_addr(&self) -> anyhow::Result<SocketAddr> {
        format!("{}:{}", self.host, self.tcp_port)
            .parse()
            .context("invalid DNS TCP address")
    }
}

impl TlsConfig {
    pub fn socket_addr(&self) -> anyhow::Result<Option<SocketAddr>> {
        if !self.enabled {
            return Ok(None);
        }

        let addr = format!("{}:{}", self.host, self.port)
            .parse()
            .context("invalid TLS bind address")?;
        Ok(Some(addr))
    }
}
