# Architecture

## Processes

```
                ┌───────────────┐        WebSocket / REST         ┌──────────────┐
                │   heron-web   │ ◄─────────────────────────────► │              │
                │ React (static)│                                 │  heron-api   │──► Webhooks
                └───────────────┘                                 │  axum        │
                                                                  │  scanner     │
 Music library ─────────────────────────────────────────────────► │  queue mgr   │
   (volume)  │ ▲                                                  └──┬───────┬───┘
             │ │ files, tags   ┌─────────────────┐                   │       │
             │ └───────────────│ heron-librarian │ ◄──── incoming    │       │
             │                 │ identify, file  │       folder      │       │
             │                 └─────────────────┘                   │       │
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
| **heron-engine** | Real-time playout: decks, mixer, crossfades, DSP, encoders and output sinks (several outputs at once, see [Encoders and outputs](outputs.md)). Reads the music library and the fallback directory, both mounted read-only. Exposes a gRPC control interface (`heron-engine ctl` is its command-line client). Holds the upcoming tracks in memory and keeps its outputs and settings on disk. **Must keep playing if every other service is down.** See [Audio engine](audio-engine.md). |
| **heron-api** | HTTP + WebSocket server: auth (local users), library, categories, rotation, queue manager, webhooks and the library scanner. Feeds the engine over gRPC. |
| **heron-analyzer** | Analysis worker pool, as its own modular service. Reads files from the music library volume, runs `heron-analysis` and stores results in MariaDB. Can be scaled independently (several replicas) and restarted without affecting the API or the air. Uses CPUs − 1 workers by default. |
| **heron-librarian** | Library management, as its own service: identifies files dropped into the incoming folder (Chromaprint, AcoustID, MusicBrainz, Cover Art Archive), tags them and files them into the library; reorganizes the library on request and undoes changes. **The only service with write access to the library**, so `heron-api` (which faces the network) keeps a read-only mount. Every move is journaled in MariaDB and finished or rolled back after a crash. See [Library management](library.md). |
| **heron-web** | Static React build. |
| **mariadb** | Database. Used by `heron-api`, `heron-analyzer` and `heron-librarian`, never by `heron-engine`. |

### Scanner and analyzer split

Built in [M1](roadmap/m1-import-analysis.md); details in [Import and analysis](analysis.md):

- The **scanner** stays in `heron-api`: it walks and watches the library, reads tags, upserts `tracks` and enqueues analysis jobs when a file is new, changed, or analyzed with an older `analysis_version`.
- **`heron-analyzer`** claims jobs from a MariaDB-backed queue (`analysis_jobs`, using `SELECT … FOR UPDATE SKIP LOCKED`), analyzes the file and writes `track_analysis`.
- A database-backed queue adds no new infrastructure (no message broker) and lets any number of analyzer replicas share the work. If a broker is ever needed, the job interface in `heron-analysis` keeps the swap local.

### Scanner and librarian

Built in [M1b](roadmap/m1b-library-management.md). The librarian records every file it moves or re-tags in `tracks` (new path, new stamp) in the same step, so the read-only scanner finds those files unchanged. They share a MariaDB named lock (`GET_LOCK`): a scan never runs in the middle of a move. Shared code (audio file detection, content hash, tags, the lock) lives in `heron-library`.

## Repository layout

```
heron-broadcaster/
├── crates/
│   ├── core/       heron-core       shared types: track, cue points, fade curves, settings
│   ├── proto/      heron-proto      gRPC definitions between api and engine
│   ├── analysis/   heron-analysis   loudness, silence, cue and fade point detection, Chromaprint fingerprints
│   ├── library/    heron-library    library files shared by scanner and librarian: tags, content hash, lock
│   ├── dsp/        heron-dsp        resampling; later EQ, compressor, expander, limiter, crossovers, AGC chain
│   ├── outputs/    heron-outputs    encoders, OutputSink trait + Icecast (later Shoutcast, HLS, file, pipe)
│   ├── rotation/   heron-rotation   categories, separation rules, clock slots, selection
│   ├── service/    heron-service    process plumbing: logging, /healthz, /metrics, shutdown, DB connect
│   ├── engine/     heron-engine     binary: decks, mixer, pacing clock, decoding, player, gRPC server, ctl
│   ├── api/        heron-api        binary: axum, auth, queue manager, webhooks, scanner
│   ├── analyzer/   heron-analyzer   binary: analysis worker pool, job queue consumer
│   └── librarian/  heron-librarian  binary: incoming folder, identification, layout, journaled moves, organize, undo
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
| Fingerprints (Chromaprint) | `rusty-chromaprint` (MIT) |
| AcoustID, MusicBrainz, Cover Art Archive | `reqwest` (rustls) |
| Renames that never replace | `rustix` (`renameat2(RENAME_NOREPLACE)`, `renamex_np(RENAME_EXCL)`) |
| Lock-free ring buffers | `rtrb` |
| Filters | `biquad` or hand-written biquads |
| MP3 / Opus / Vorbis encoding | LAME (`mp3lame-encoder`, built from source and linked statically), libopus, libvorbis bindings |
| AAC-LC encoding | FFmpeg's native `aac` encoder via libavcodec (no `--enable-nonfree`, e.g. `ffmpeg-next` bindings) |
| HLS segmenting | Hand-written segmenter, or libavformat's `hls` muxer |
| HTTP / WebSocket | `axum`, `tokio` |
| gRPC | `tonic`, `prost`; `protox` compiles the `.proto` files in Rust, so builds need no `protoc` |
| Database | `sqlx` (MySQL/MariaDB) |
| Logging / metrics | `tracing`, `metrics` + Prometheus exporter |
| Optional monitor output | `cpal` |

These are suggestions: replace one if there is a good reason, and say why. Codec choices are constrained by the license: Heron never ships or links fdk-aac, and any FFmpeg libraries it links must be built without `--enable-nonfree` (see [Locked decisions](decisions.md)).
