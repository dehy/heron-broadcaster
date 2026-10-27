# Heron Broadcaster documentation

Heron Broadcaster ("Heron" for short) is a modern, self-hosted **radio automation system** for Linux. It plays music 24/7 to a streaming server (Icecast first), schedules tracks with category-based clockwheels, crossfades between tracks, removes silences, processes audio (EQ, AGC, multiband compression) and is fully operated from a web interface.

The name comes from "air on" sounding like *héron* (French for heron): keeping the air on, 24/7.

## Start here

| Document | Contents |
|---|---|
| [Workflow](workflow.md) | How to work on the project, milestone by milestone |
| [Overview](overview.md) | Project identity: name, domain, license, crate and binary names |
| [Locked decisions](decisions.md) | Choices that are fixed and must not be worked around |

## Product

| Document | Contents |
|---|---|
| [Requirements](requirements.md) | Library, playout, audio processing, outputs, reliability |
| [Rotation](rotation.md) | Separation rules, clockwheels, relaxation, rotation model |
| [Import and analysis](analysis.md) | Library scanner, job queue, silence and crossfade detection |
| [Importing from another software](import.md) | Bringing another radio software's tags, categories, fades, intros and hooks into the library, with a dry run, conflicts and undo |
| [Library management](library.md) | Incoming folder, buffer zone and uploads, identification, tags, layout, journaled moves, organize, trash and undo |
| [Encoders and outputs](outputs.md) | Encoder selection, multiple outputs, Icecast, Shoutcast, HLS |
| [Listener statistics](statistics.md) | Listeners per day, hour, output and track from the stream servers, sessions, privacy, CSV export |

## Technical design

| Document | Contents |
|---|---|
| [Architecture](architecture.md) | Processes, repository layout, suggested libraries |
| [Audio engine](audio-engine.md) | How `heron-engine` plays: threads, transitions, playout settings, control interface, real-time rules |
| [Engine handover](handover.md) | How a new engine takes over from the running one: protocol, exact stop, connections passed over a Unix socket, failures |
| [API and control plane](api.md) | `heron-api`: authentication, routes, live WebSocket, queue manager, play history, engine configuration, library tasks, OpenAPI |
| [Web UI](web.md) | `heron-web`: pages, live updates, waveform and previews, themes, development |
| [Data model](data-model.md) | Initial database schema sketch |
| [Monitoring](monitoring.md) | Metrics to Prometheus or Grafana Cloud (remote write), dashboard and alerts to import, health checks, logs, Sentry, metrics reference |
| [Upgrading](upgrading.md) | Upgrading a station on air with Docker Compose or Swarm: hot engine handover, Icecast fallback mount |
| [Configuration backups](backups.md) | The station's configuration saved every day to a file, the last 7 days kept, restored whole from the web UI or the command line |
| [Conventions](conventions.md) | Code style, errors, logging, testing, commits |

## Planning

| Document | Contents |
|---|---|
| [Roadmap](roadmap/README.md) | Milestones M0 to M11, dependencies and acceptance criteria |
| [Open questions](open-questions.md) | Decisions still to be made |
