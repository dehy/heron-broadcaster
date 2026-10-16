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
