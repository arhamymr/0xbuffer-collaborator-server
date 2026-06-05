# 0xbuffer Collaborator Server

Self-hosted out-of-band interaction collection for 0xbuffer Desktop.

## Features

- Bearer-token REST API for payloads, interactions, health, metrics, and statistics
- SQLite persistence
- HTTP callback listener
- DNS UDP/TCP callback listener
- Optional Rustls HTTPS listener
- Docker Compose deployment

## Run Locally

```bash
cargo run
```

The development defaults avoid privileged ports:

- API: `http://127.0.0.1:8080`
- HTTP callbacks: `http://127.0.0.1:8081`
- DNS callbacks: `127.0.0.1:1053`

Create a payload:

```bash
curl -s \
  -H 'Authorization: Bearer change_me' \
  -H 'Content-Type: application/json' \
  -d '{"name":"demo","tags":["local"]}' \
  http://127.0.0.1:8080/api/v1/payloads
```

List interactions:

```bash
curl -s -H 'Authorization: Bearer change_me' \
  http://127.0.0.1:8080/api/v1/interactions
```

## Docker Compose Deployment

```bash
cp .env.example .env
$EDITOR .env
docker compose up -d
```

Deployment pieces:

- Docker builds the Rust collaborator server.
- Docker Compose runs the collaborator and Caddy.
- Caddy publishes ports `80` and `443`.
- DNS callbacks are exposed directly on port `53/udp` and `53/tcp`.
- SQLite is stored in the `collaborator-sqlite` named volume at `/data/0xbuffer.db`.
- `.env` supplies `COLLAB_DOMAIN`, `API_DOMAIN`, `ACME_EMAIL`, and `APP_API_KEY`.

For production, point these records at the host public IP:

```text
api.collab.company.com
*.collab.company.com
```

The default Caddyfile proxies the API hostname with automatic HTTPS and proxies wildcard callback hosts over HTTP. Wildcard HTTPS for payload callbacks requires a Caddy DNS-challenge build/plugin, because public CAs do not issue wildcard certificates via HTTP challenge.

## Configuration

Configuration is loaded from `config/default.yaml` and can be overridden with environment variables using `APP__` and double underscores for nesting.

Example:

```bash
APP__DOMAIN__ROOT=collab.company.com
APP__SECURITY__API_KEY=replace_me
APP__TLS__ENABLED=true
APP__TLS__CERT=/certs/fullchain.pem
APP__TLS__KEY=/certs/privkey.pem
```
