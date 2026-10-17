# Architecture

## Processes

```
                ┌───────────────┐        WebSocket / REST         ┌──────────────┐
                │   heron-web   │ ◄─────────────────────────────► │              │
                │ React (static)│                                 │  heron-api   │──► Webhooks
                └───────────────┘                                 │  axum        │
                                                                  │  scanner     │
 Music library ─────────────────────────────────────────────────► │  queue mgr   │
   (volume)  │                                                    └──┬───────┬───┘
             │                                                       │       │ gRPC (tonic)
             │         ┌────────────────┐   analysis jobs / results  │       ▼
             └───────► │ heron-analyzer │ ◄────────► MariaDB ◄───────┘ ┌──────────────┐
                       │ worker pool    │                              │ heron-engine │──► encoders ──► outputs
                       └────────────────┘     fallback directory ───►  │ decks, mixer │     MP3      ──► Icecast A, B
                                                                       │ DSP,encoders │     AAC-LC   ──► Icecast C, HLS
                                                                       └──────────────┘     ffmpeg*  ──► Shoutcast D
                                                                                            (* user-provided binary)
```

| Service | Role |
|---|---|
| **heron-engine** | Real-time playout: decks, mixer, crossfades, DSP, encoders and output sinks (several outputs at once, see [Encoders and outputs](outputs.md)). Exposes a gRPC control interface. Holds the upcoming tracks in memory. **Must keep playing if every other service is down.** See [Audio engine](audio-engine.md). |
| **heron-api** | HTTP + WebSocket server: auth (local users), library, categories, rotation, queue manager, webhooks and the library scanner. Feeds the engine over gRPC. |
| **heron-analyzer** | Analysis worker pool, as its own modular service. Reads files from the music library volume, runs `heron-analysis` and stores results in MariaDB. Can be scaled independently (several replicas) and restarted without affecting the API or the air. |
| **heron-web** | Static React build. |
| **mariadb** | Database. Used by `heron-api` and `heron-analyzer`, never by `heron-engine`. |

### Scanner and analyzer split

Proposed for [M1](roadmap/m1-import-analysis.md), to be confirmed when it is built:

- The **scanner** stays in `heron-api`: it walks and watches the library, reads tags, upserts `tracks` and enqueues analysis jobs when a file is new, changed, or analyzed with an older `analysis_version`.
- **`heron-analyzer`** claims jobs from a MariaDB-backed queue (`analysis_jobs`, using `SELECT … FOR UPDATE SKIP LOCKED`), analyzes the file and writes `track_analysis`.
- A database-backed queue adds no new infrastructure (no message broker) and lets any number of analyzer replicas share the work. If a broker is ever needed, the job interface in `heron-analysis` keeps the swap local.

## Repository layout

```
heron-broadcaster/
├── crates/
│   ├── core/       heron-core       shared types: track, cue points, fade curves, settings
│   ├── proto/      heron-proto      gRPC definitions between api and engine
│   ├── analysis/   heron-analysis   loudness, silence, cue and fade point detection
│   ├── dsp/        heron-dsp        EQ, compressor, expander, limiter, crossovers, AGC chain
│   ├── outputs/    heron-outputs    encoders, OutputSink trait + Icecast (later Shoutcast, HLS, file, pipe)
│   ├── rotation/   heron-rotation   categories, separation rules, clock slots, selection
│   ├── service/    heron-service    process plumbing: logging, /healthz, /metrics, shutdown, DB connect
│   ├── engine/     heron-engine     binary: decks, mixer, pacing clock, buffering, recovery
│   ├── api/        heron-api        binary: axum, auth, queue manager, webhooks, scanner
│   └── analyzer/   heron-analyzer   binary: analysis worker pool, job queue consumer
├── web/                             React + Vite + TypeScript
├── migrations/                      sqlx migrations
├── docker/                          Dockerfiles
├── docker-compose.yml
├── LICENSE                          AGPL-3.0
└── README.md
```

## Suggested libraries

| Need | Crate / library |
|---|---|
| Decoding | `symphonia` |
| Resampling | `rubato` |
| Loudness (EBU R128) | `ebur128` |
| Tags | `lofty` |
| Lock-free ring buffers | `rtrb` |
| Filters | `biquad` or hand-written biquads |
| MP3 / Opus / Vorbis encoding | LAME, libopus, libvorbis bindings |
| AAC-LC encoding | FFmpeg's native `aac` encoder via libavcodec (no `--enable-nonfree`, e.g. `ffmpeg-next` bindings) |
| HLS segmenting | Hand-written segmenter, or libavformat's `hls` muxer |
| HTTP / WebSocket | `axum`, `tokio` |
| gRPC | `tonic`, `prost` |
| Database | `sqlx` (MySQL/MariaDB) |
| Logging / metrics | `tracing`, `metrics` + Prometheus exporter |
| Optional monitor output | `cpal` |

These are suggestions: replace one if there is a good reason, and say why. Codec choices are constrained by the license: Heron never ships or links fdk-aac, and any FFmpeg libraries it links must be built without `--enable-nonfree` (see [Locked decisions](decisions.md)).
