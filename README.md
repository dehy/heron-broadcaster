# Heron Broadcaster

A modern, self-hosted **radio automation system** for Linux. Heron plays music 24/7 to your stream servers, schedules tracks with category-based clockwheels, crossfades and removes silences, processes audio, and is operated entirely from a web interface.

The name comes from "air on" sounding like *héron*: keeping the air on, 24/7.

> **Status:** early development. Heron imports and analyzes a music library (M1, M1b), plays a queue with crossfades to Icecast servers (M2), and is driven through its API (M4), which keeps the queue filled with the least recently played tracks until rotation and clockwheels arrive (M6). The web interface comes with M5. See the [roadmap](docs/roadmap/README.md).

Heron runs on Linux, amd64 and arm64, down to a Raspberry Pi.

## Quick start

Requirements: Docker with Compose v2.

```sh
cp .env.example .env              # then change the passwords
mkdir -p data/music data/fallback data/incoming # or point HERON_MUSIC_DIR, … elsewhere
docker compose up -d --build --wait
```

Open http://localhost:8000. The page shows the service status and the import progress. Database migrations run automatically when `heron-api` starts.

Point `HERON_MUSIC_DIR` at your music: new and changed files are picked up automatically. See [Import and analysis](docs/analysis.md).

Drop new files into `HERON_INCOMING_FOLDER` (`data/incoming`): `heron-librarian` identifies them (with your station's AcoustID key), tags them and files them into the library as `Artist/Album/01 Title.ext`. It never overwrites a file, and reorganizes your existing library only when you ask, after a dry run. Set `HERON_LIBRARIAN_USER` to the owner of your music (`id -u`:`id -g`), the only user Heron writes as. See [Library management](docs/library.md).

To try it without your own music, generate a synthetic library (uses ffmpeg in Docker):

```sh
scripts/generate-test-library.sh data/music 100
```

### Going on air

Create the first admin, then use the API (the web interface comes with M5). With the local Icecast server of the `icecast` profile:

```sh
docker compose --profile icecast up -d --wait
docker compose exec heron-api heron-api user add admin --role admin   # asks for a password

api=http://localhost:8000/api/v1
curl -c cookies -H 'Content-Type: application/json' -d '{"username": "admin", "password": "…"}' $api/auth/login
curl -b cookies -H 'X-Requested-With: curl' -H 'Content-Type: application/json' $api/outputs -d \
  '{"name": "Local", "kind": "icecast", "encoder_profile_id": 1,
    "config": {"host": "icecast", "port": 8000, "mount": "/live", "password": "hackme"}}'
```

Listen on http://localhost:8010/live: the queue manager keeps three tracks queued. `GET /api/v1/player`, `POST /api/v1/player/skip`, `GET /api/v1/history` and the rest are in [API and control plane](docs/api.md) and in the OpenAPI document at http://localhost:8000/api/v1/openapi.json.

| Service | Role | Port (inside the network) |
|---|---|---|
| `heron-web` | Web interface, proxies `/api` to `heron-api` | 80, published on `HERON_HTTP_PORT` (8000) |
| `heron-api` | HTTP and WebSocket API, migrations, library scanner, queue manager | 8080 |
| `heron-librarian` | Incoming folder, identification, tags, library layout | 8083 |
| `heron-analyzer` | Track analysis workers | 8082 |
| `heron-engine` | Real-time playout, encoders, outputs | 8081, gRPC control on 50051 |
| `mariadb` | Database | 3306 |
| `icecast` | Optional local Icecast server (`--profile icecast`) | 8000, published on `HERON_ICECAST_PORT` (8010) |

Every Rust service serves `/healthz` and `/metrics` (Prometheus) on its port.

## Development

Requirements: Rust (the toolchain in `rust-toolchain.toml` installs automatically through rustup), Node.js 22, Docker.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Database tests create and drop scratch databases, so they need an admin URL:
docker run -d --rm --name heron-test-db -p 127.0.0.1:33306:3306 \
  -e MARIADB_ROOT_PASSWORD=root mariadb:11.8
HERON_TEST_DATABASE_URL=mysql://root:root@127.0.0.1:33306 cargo test --workspace

# Soak test of the engine: two Icecast servers, the library for an hour:
cargo build --release -p heron-engine
scripts/soak-test.sh data/music 3600

# Analyze files with given settings, without a database:
HERON_ANALYSIS_SETTINGS='{"silence_ceiling_dbfs": -60}' \
  cargo run --release -p heron-analysis --example analyze -- path/to/*.mp3

# Web app, with /api proxied to a local heron-api on :8080:
cd web && npm install && npm run dev
```

Running a binary locally: `cargo run -p heron-api -- --help`. All options can also be set through `HERON_*` environment variables.

## Documentation

Start with [docs/README.md](docs/README.md): requirements, architecture, data model, conventions and the milestone roadmap.

## Built with Claude

Heron Broadcaster is written with [Claude](https://claude.com/claude-code), Anthropic's AI coding assistant. This is a deliberate choice, and we stand behind it openly. A human maintainer sets the direction and makes the decisions ([docs/decisions.md](docs/decisions.md)), and reviews every change. Commits written with Claude carry a `Co-Authored-By: Claude` trailer.

## License

Heron Broadcaster is free software, licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).
