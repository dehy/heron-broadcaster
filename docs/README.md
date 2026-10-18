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
| [Library management](library.md) | Incoming folder, buffer zone and uploads, identification, tags, layout, journaled moves, organize, trash and undo |
| [Encoders and outputs](outputs.md) | Encoder selection, multiple outputs, Icecast, Shoutcast, HLS |

## Technical design

| Document | Contents |
|---|---|
| [Architecture](architecture.md) | Processes, repository layout, suggested libraries |
| [Audio engine](audio-engine.md) | How `heron-engine` plays: threads, transitions, playout settings, control interface, real-time rules |
| [API and control plane](api.md) | `heron-api`: authentication, routes, live WebSocket, queue manager, play history, engine configuration, library tasks, OpenAPI |
| [Web UI](web.md) | `heron-web`: pages, live updates, waveform and previews, themes, development |
| [Data model](data-model.md) | Initial database schema sketch |
| [Monitoring](monitoring.md) | Prometheus and Grafana (dashboard, alerts), health checks, logs, Sentry, metrics reference |
| [Conventions](conventions.md) | Code style, errors, logging, testing, commits |

## Planning

| Document | Contents |
|---|---|
| [Roadmap](roadmap/README.md) | Milestones M0 to M10, dependencies and acceptance criteria |
| [Open questions](open-questions.md) | Decisions still to be made |
