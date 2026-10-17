# Locked decisions

These decisions are fixed. If one looks wrong, stop and ask instead of working around it.

| Area | Decision |
|---|---|
| Language | **Rust** for the engine, analysis, rotation and API. |
| Database | **MariaDB**, accessed with `sqlx` (MySQL driver). Migrations live in the repository and run automatically on startup. |
| Frontend | **React + Vite + TypeScript** (a web app, not React Native). |
| Deployment | **Docker Compose**, one service per process. |
| Analysis | Loudness, cue points, silence trimming and crossfade points are computed **when a track is imported** and stored in the database. Never during playback. |
| Clockwheels | Clockwheels are **data**: ordered slots stored in the database and edited in the UI. No scripting language for now. |
| Audio path | **The engine never touches the database in the audio path.** |
| Roadmap order | Audio processing (AGC, multiband) **comes last** in the roadmap. |
| Outputs | **Bring your own streamer:** outputs are pluggable sinks behind a trait. Icecast first. |
| Codecs | **Heron never ships or links fdk-aac** (license incompatible with AGPL). LAME (LGPL), libopus and libvorbis (BSD) are fine. AAC-LC is built in using FFmpeg's native `aac` encoder (LGPL). fdk-aac / HE-AAC only through a user-provided `ffmpeg` binary. *(Revised after the handoff, see below.)* |

## Decisions made after the handoff

| Area | Decision |
|---|---|
| Authentication | **Local users only for now** (users and sessions or API tokens in MariaDB). OIDC may be added later. |
| Analysis worker | The analysis worker is **its own modular service**, `heron-analyzer`, not part of `heron-api`. See [Architecture](architecture.md). |
| Multiple outputs | A station can stream to **several stream servers at once**, each output with its own encoder profile. See [Encoders and outputs](outputs.md). |
| Encoder selection | The user **selects the encoder** per profile: MP3, AAC-LC, Opus, Vorbis (built in) or external FFmpeg. This replaces the generic "pipe to ffmpeg" approach for encoding. |
| AAC | **Built-in AAC-LC without HE is sufficient.** fdk-aac (HE-AAC) is only available when the user provides their own fdk-aac-enabled `ffmpeg` binary; Heron does not ship it. |
| HLS | Heron has an **HLS output** (segments + playlist). |
| Rotation relaxation | Album → artist → title → track separation, each halved before being dropped (track only reduced to a quarter, never dropped), then the fallback category. See [Rotation](rotation.md#rule-relaxation). |
| Default encoder profiles | `MP3 128k` (CBR, 44.1 kHz, stereo) and `AAC 96k` (AAC-LC, 48 kHz, stereo). Opus is optional. See [Encoders and outputs](outputs.md#default-profiles). |
| UI theme | The web UI has light and dark themes and **follows the system setting** by default. |
| Platforms | Heron must **run on a Raspberry Pi**. Every image is built for **amd64 and arm64**, and CI tests both natively. Defaults leave CPU headroom for the engine (the analyzer uses CPUs − 1 workers). |
| Library files | Heron **manages the library**: files dropped into an incoming folder are identified, tagged and filed under Artist / Album / Track. Moves are journaled and can be undone; files are never overwritten or deleted. An existing library is reorganized only on request, after a dry run. Until [M1b](roadmap/m1b-library-management.md) ships, the library is mounted read-only. Track paths are stored relative to the library root. |
| Track identification | AcoustID fingerprint lookup, then MusicBrainz. Matches are **applied automatically when confident**, otherwise they wait for review in the web UI. See [M1b](roadmap/m1b-library-management.md). |
| Silence detection | Gap Killer silence must be below the relative thresholds **and** below an absolute ceiling of −50 dBFS, so soft intros are kept. See [Import and analysis](analysis.md#what-counts-as-silence). |
