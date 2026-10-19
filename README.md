# Heron Broadcaster

A modern, self-hosted **radio automation system** for Linux. Heron plays music 24/7 to your stream servers, schedules tracks with category-based clockwheels, crossfades and removes silences, processes audio, and is operated entirely from a web interface.

The name comes from "air on" sounding like *héron*: keeping the air on, 24/7.

![The On air page: decks A and B, the next tracks, levels, the FX player, outputs and the engine's state](docs/images/on-air.png)

> **Status:** early development, but it plays. Heron imports and analyzes a music library, files new music into it, plays a queue with crossfades to Icecast servers, picks the music with categories, clocks and a weekly schedule, and is operated from a web interface in English or French. Next: 24/7 reliability (M3), track details and management in the browser (M5b, M5c), webhooks (M7) and audio processing (M8, M9). See the [roadmap](docs/roadmap/README.md).

Heron runs on Linux, amd64 and arm64, down to a Raspberry Pi.

## What it does today

- **Library:** watches your music folder (MP3, AAC and ALAC in M4A, FLAC, Ogg Vorbis, WAV, AIFF) and analyzes every track: loudness, true peak, silences at both ends, where the next track can start, a waveform. See [Import and analysis](docs/analysis.md).
- **Incoming folder:** new files are identified (AcoustID, MusicBrainz), tagged and filed as `Artist/Album/01 Title.ext`; doubtful matches wait for review. Reorganizing the library is a dry run first, journaled, and can be undone. See [Library management](docs/library.md).
- **Playout:** two decks with crossfades or fixed overlaps, Gap Killer, loudness normalization, an FX player for jingles over the music, a fallback folder, MP3 to one or more Icecast servers with track metadata. See [Audio engine](docs/audio-engine.md) and [Encoders and outputs](docs/outputs.md).
- **Rotation:** categories, clocks of slots, a weekly schedule, separation rules (title, artist, album) relaxed step by step when the library runs short, artists with several names, restrictions (dayparting, date ranges, sequences) that are never relaxed, and a selection log that explains every pick. A simulation checks a day or a week ahead. See [Rotation](docs/rotation.md).
- **Web interface:** on air, queue, history, tracks with their waveform and cue points, artists, clocks, schedule, restrictions, outputs and settings; live updates, light and dark themes, English and French, from phone width up. Viewer, operator and admin roles. See [Web UI](docs/web.md).
- **API:** everything the web interface does goes through a documented HTTP and WebSocket API (OpenAPI), with API tokens for scripts. See [API and control plane](docs/api.md).

## Screenshots

| | |
|---|---|
| [![A clock: its slots, the hour drawn as it will sound, a simulation](docs/images/clock-editor.png)](docs/images/clock-editor.png) **Clock editor:** slots by drag and drop, the hour as it will sound | [![A track: waveform with cue-in, cue-out and where the next track starts](docs/images/track.png)](docs/images/track.png) **Track:** waveform, cue points and crossfade, analysis |
| [![The week, hour by hour, painted with clocks](docs/images/schedule.png)](docs/images/schedule.png) **Schedule:** which clock runs at each hour of the week | [![The On air page in the dark theme](docs/images/on-air-dark.png)](docs/images/on-air-dark.png) **Dark theme** |

<p align="center"><img src="docs/images/on-air-phone.png" alt="The On air page at phone width" width="260"></p>

The screenshots show a demo station with generated music by fictional artists.

## Quick start

Requirements: Docker with Compose v2.

```sh
cp .env.example .env              # then change the passwords
mkdir -p data/music data/fallback data/incoming # or point HERON_MUSIC_DIR, … elsewhere
docker compose up -d --build --wait
docker compose exec heron-api heron-api user add admin --role admin   # asks for a password
```

Open http://localhost:8000 and sign in. Database migrations run automatically when `heron-api` starts.

Point `HERON_MUSIC_DIR` at your music: new and changed files are picked up automatically, and the **Tracks** page shows them as they are analyzed. See [Import and analysis](docs/analysis.md).

Drop new files into `HERON_INCOMING_FOLDER` (`data/incoming`): `heron-librarian` identifies them (with your station's AcoustID key, in **Settings**), tags them and files them into the library. It never overwrites a file, and reorganizes your existing library only when you ask, after a dry run. Set `HERON_LIBRARIAN_USER` to the owner of your music (`id -u`:`id -g`), the only user Heron writes as. See [Library management](docs/library.md).

To try it without your own music, generate a synthetic library (uses ffmpeg in Docker):

```sh
scripts/generate-test-library.sh data/music 100
```

### Going on air

Heron starts playing as soon as tracks are analyzed: with autofill on (the default), the queue manager keeps three tracks queued, picked by rotation. To hear it, start the local Icecast server of the `icecast` profile:

```sh
docker compose --profile icecast up -d --wait
```

Then, in the web interface, open **Outputs** and add an Icecast output: host `icecast`, port `8000`, mount `/live`, password `hackme`, with the MP3 profile. Listen on http://localhost:8010/live.

To shape the programme, create **Categories**, put tracks in them (from the track's page), build **Clocks** from them and paint the week in **Schedule**. Hours without a clock rotate the whole library.

The same can be done from scripts: every route is in [API and control plane](docs/api.md) and in the OpenAPI document at http://localhost:8000/api/v1/openapi.json.

```sh
api=http://localhost:8000/api/v1
curl -c cookies -H 'Content-Type: application/json' -d '{"username": "admin", "password": "…"}' $api/auth/login
curl -b cookies $api/player
curl -b cookies -X POST -H 'X-Requested-With: curl' $api/player/skip
```

### Services

| Service | Role | Port (inside the network) |
|---|---|---|
| `heron-web` | Web interface, proxies `/api` to `heron-api` | 80, published on `HERON_HTTP_PORT` (8000) |
| `heron-api` | HTTP and WebSocket API, migrations, library scanner, queue manager, rotation | 8080 |
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
cd web && npm ci && npm run dev
npm test && npm run typecheck   # unit tests (translation catalogs included) and types
```

Running a binary locally: `cargo run -p heron-api -- --help`. All options can also be set through `HERON_*` environment variables. Adding a language to the web interface takes one catalog file: see [Web UI – Adding a language](docs/web.md#adding-a-language).

## Documentation

Start with [docs/README.md](docs/README.md): requirements, architecture, data model, conventions and the milestone roadmap.

## Built with Claude

Heron Broadcaster is written with [Claude](https://claude.com/claude-code), Anthropic's AI coding assistant. This is a deliberate choice, and we stand behind it openly. A human maintainer sets the direction and makes the decisions ([docs/decisions.md](docs/decisions.md)), and reviews every change. Commits written with Claude carry a `Co-Authored-By: Claude` trailer.

## License

Heron Broadcaster is free software, licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).
