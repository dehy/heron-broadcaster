# Heron Broadcaster

A modern, self-hosted **radio automation system** for Linux. Heron plays music 24/7 to your stream servers, schedules tracks with category-based clockwheels, crossfades and removes silences, processes audio, and is operated entirely from a web interface.

The name comes from "air on" sounding like *héron*: keeping the air on, 24/7.

> **Status:** early development. Milestone M0 (foundations) provides the services, the database schema and CI; playout arrives in M2. See the [roadmap](docs/roadmap/README.md).

## Quick start

Requirements: Docker with Compose v2.

```sh
cp .env.example .env              # then change the passwords
mkdir -p data/music data/fallback # or point HERON_MUSIC_DIR / HERON_FALLBACK_DIR elsewhere
docker compose up -d --build --wait
```

Open http://localhost:8000. The page shows the API and database status. Database migrations run automatically when `heron-api` starts.

| Service | Role | Port (inside the network) |
|---|---|---|
| `heron-web` | Web interface, proxies `/api` to `heron-api` | 80, published on `HERON_HTTP_PORT` (8000) |
| `heron-api` | HTTP API, migrations, library scanner, queue manager | 8080 |
| `heron-analyzer` | Track analysis workers | 8082 |
| `heron-engine` | Real-time playout, encoders, outputs | 8081 |
| `mariadb` | Database | 3306 |

Every Rust service serves `/healthz` and `/metrics` (Prometheus) on its port.

## Development

Requirements: Rust (the toolchain in `rust-toolchain.toml` installs automatically through rustup), Node.js 22, Docker.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Migration test against a throwaway MariaDB:
docker run -d --rm --name heron-test-db -p 127.0.0.1:33306:3306 \
  -e MARIADB_ROOT_PASSWORD=root -e MARIADB_DATABASE=heron_test \
  -e MARIADB_USER=heron -e MARIADB_PASSWORD=heron mariadb:11.8
HERON_TEST_DATABASE_URL=mysql://heron:heron@127.0.0.1:33306/heron_test cargo test --workspace

# Web app, with /api proxied to a local heron-api on :8080:
cd web && npm install && npm run dev
```

Running a binary locally: `cargo run -p heron-api -- --help`. All options can also be set through `HERON_*` environment variables.

## Documentation

Start with [docs/README.md](docs/README.md): requirements, architecture, data model, conventions and the milestone roadmap.

## Built with Claude

Heron Broadcaster is written with [Claude](https://claude.com/claude-code), Anthropic's AI coding assistant, and we say so openly. A human maintainer sets the direction and makes the decisions ([docs/decisions.md](docs/decisions.md)), and reviews every change. Commits written with Claude carry a `Co-Authored-By: Claude` trailer.

## License

Heron Broadcaster is free software, licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).
