use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
};
use tracing::{info, warn};

use crate::{models::NewInteraction, payloads, state::AppState};

const MAX_DNS_PACKET: usize = 512;

pub async fn serve_udp(addr: SocketAddr, state: Arc<AppState>) -> anyhow::Result<()> {
    let socket = UdpSocket::bind(addr)
        .await
        .with_context(|| format!("failed to bind dns udp at {addr}"))?;
    info!(%addr, "starting dns-udp-listener");

    let mut buf = [0_u8; MAX_DNS_PACKET];
    loop {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        let packet = &buf[..len];
        let response = handle_packet(packet, peer, state.clone()).await;
        if let Some(response) = response {
            socket.send_to(&response, peer).await?;
        }
    }
}

pub async fn serve_tcp(addr: SocketAddr, state: Arc<AppState>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind dns tcp at {addr}"))?;
    info!(%addr, "starting dns-tcp-listener");

    loop {
        let (stream, peer) = listener.accept().await?;
        tokio::spawn(handle_tcp_stream(stream, peer, state.clone()));
    }
}

async fn handle_tcp_stream(mut stream: TcpStream, peer: SocketAddr, state: Arc<AppState>) {
    let mut len_buf = [0_u8; 2];
    if stream.read_exact(&mut len_buf).await.is_err() {
        return;
    }
    let len = u16::from_be_bytes(len_buf) as usize;
    if len > 4096 {
        return;
    }

    let mut packet = vec![0_u8; len];
    if stream.read_exact(&mut packet).await.is_err() {
        return;
    }

    if let Some(response) = handle_packet(&packet, peer, state).await {
        let len = (response.len() as u16).to_be_bytes();
        let _ = stream.write_all(&len).await;
        let _ = stream.write_all(&response).await;
    }
}

async fn handle_packet(packet: &[u8], peer: SocketAddr, state: Arc<AppState>) -> Option<Vec<u8>> {
    let query = match parse_query(packet) {
        Ok(query) => query,
        Err(error) => {
            warn!(?error, "invalid DNS query");
            return build_response(packet, 1);
        }
    };

    if let Some(payload_id) = payloads::extract_payload_id(&query.name, &state.config.domain.root) {
        match state.database.payload_exists_and_active(&payload_id).await {
            Ok(true) => {
                let interaction = NewInteraction {
                    payload_id,
                    interaction_type: "dns_query".to_string(),
                    source_ip: Some(peer.ip().to_string()),
                    protocol: "dns".to_string(),
                    method: None,
                    path: Some(query.name.clone()),
                    query_type: Some(query.query_type.to_string()),
                    headers: json!({}),
                    body: None,
                    tls_metadata: json!({}),
                };
                if let Err(error) = state.database.insert_interaction(interaction).await {
                    warn!(?error, "failed to store DNS interaction");
                }
            }
            Ok(false) => {}
            Err(error) => warn!(?error, "failed to validate DNS payload"),
        }
    }

    build_response(packet, 3)
}

fn build_response(query: &[u8], rcode: u8) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }

    let mut response = query.to_vec();
    response[2] = 0b1000_0001;
    response[3] = 0b1000_0000 | (rcode & 0x0f);
    response[6] = 0;
    response[7] = 0;
    response[8] = 0;
    response[9] = 0;
    response[10] = 0;
    response[11] = 0;
    Some(response)
}

struct DnsQuery {
    name: String,
    query_type: u16,
}

fn parse_query(packet: &[u8]) -> anyhow::Result<DnsQuery> {
    if packet.len() < 12 {
        anyhow::bail!("DNS packet too short");
    }

    let qdcount = u16::from_be_bytes([packet[4], packet[5]]);
    if qdcount == 0 {
        anyhow::bail!("DNS packet has no questions");
    }

    let mut offset = 12;
    let mut labels = Vec::new();
    loop {
        if offset >= packet.len() {
            anyhow::bail!("DNS name exceeds packet");
        }
        let len = packet[offset] as usize;
        offset += 1;
        if len == 0 {
            break;
        }
        if len & 0b1100_0000 != 0 {
            anyhow::bail!("compressed names are not accepted in questions");
        }
        if offset + len > packet.len() {
            anyhow::bail!("DNS label exceeds packet");
        }
        labels.push(String::from_utf8_lossy(&packet[offset..offset + len]).into_owned());
        offset += len;
    }

    if offset + 4 > packet.len() {
        anyhow::bail!("DNS question is truncated");
    }

    let query_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
    Ok(DnsQuery {
        name: labels.join("."),
        query_type,
    })
}
