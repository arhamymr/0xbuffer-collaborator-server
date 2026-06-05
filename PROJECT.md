# 0xbuffer Collaborator Server Project

## Summary

0xbuffer Collaborator Server is a self-hosted out-of-band interaction collector for 0xbuffer Desktop. It generates callback payloads and records inbound DNS and HTTP interactions for later evidence retrieval.

## Stack

- Language: Rust
- Runtime: Tokio
- HTTP API: Axum
- Database: SQLite via SQLx
- Reverse proxy: Caddy
- Deployment: Docker Compose
- Logging: tracing
- Authentication: Bearer API key

## Services

- API service: payload management, interaction retrieval, health, metrics, statistics
- HTTP listener: captures callback requests for generated payload hostnames
- DNS listener: captures DNS lookup callbacks over UDP and TCP
- Database: stores payloads and interactions in SQLite
- Caddy: publishes HTTP/HTTPS entrypoints and proxies traffic to the collaborator service

## Deployment Files

- `Dockerfile`: builds the Rust binary and runtime image
- `docker-compose.yml`: runs the collaborator service, Caddy, DNS ports, and named volumes
- `deploy/caddy/Caddyfile`: Caddy reverse proxy configuration
- `.env.example`: deployment environment template
- `.env`: local deployment configuration, ignored by git

## Persistent Data

SQLite data is stored in the Docker named volume:

```text
collaborator-sqlite
```

Inside the container, the database path is:

```text
/data/0xbuffer.db
```

## Required DNS

Point these records to the deployment host public IP:

```text
api.collab.company.com
*.collab.company.com
```

## Run

```bash
cp .env.example .env
docker compose up -d
```

## API Auth

API requests require:

```http
Authorization: Bearer APP_API_KEY
```

## Important Endpoints

```text
GET  /health
GET  /metrics
POST /api/v1/payloads
GET  /api/v1/payloads
GET  /api/v1/interactions
GET  /api/v1/interactions/{id}
GET  /api/v1/statistics
```

## Notes

The default Caddy configuration supports automatic HTTPS for the API hostname. Wildcard HTTPS for payload callbacks requires a Caddy DNS-challenge build/plugin for the DNS provider.
