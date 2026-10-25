# M8 – Hot engine upgrades

**Depends on:** M3

## Why

Upgrading Heron replaces the `heron-engine` container. Since [M3](m3-reliability.md) the *programme* survives it: the queue is saved on SIGTERM and the new engine resumes the track on air where it was. The *stream* does not: while no engine runs (stop, image pull in Swarm, start, output connection), Icecast has no source on the mount and disconnects every listener, and many players do not reconnect on their own.

The other services can already be upgraded on air: the engine keeps playing without them.

## Scope

Two steps, usable separately:

1. **Listeners stay connected through a restart** (configuration and procedure, no engine code):
   - An Icecast fallback mount, fed by a looping source in the stream's format, with `fallback-override`: listeners move to it when the engine's source leaves and back when it returns. Shipped as `docker/icecast/icecast.xml` and an `icecast-fallback` service in the `icecast` profile of `docker-compose.yml`. See [Upgrading – Icecast fallback mount](../upgrading.md#icecast-fallback-mount).
   - An upgrade procedure for Docker Compose and Docker Swarm: pull first, upgrade the other services at any time, the engine at a chosen moment ([Upgrading](../upgrading.md)).
2. **No gap at all: hot handover between engines.** The new engine starts next to the old one as a **standby**. On SIGTERM, the old engine hands it the queue, the exact play position and its open output connections (file descriptors passed over a unix socket in the state volume), and the new engine carries on from an agreed sample on the same connections. Icecast never sees the source leave. See [Audio engine – Upgrades and handover](../audio-engine.md#upgrades-and-handover).
   - **Docker Compose:** `scripts/upgrade.sh`, which upgrades every service, the engine by starting a second replica and stopping the old one. A plain `docker compose up -d` stays safe (a cold restart, step 1's fallback).
   - **Docker Swarm:** `start-first` updates for `heron-engine`; the same script pre-pulls and runs `docker service update`.

## Out of scope

- Several engines on different machines (active/passive failover across hosts): the handover passes sockets, so both engines run on the same host and share the state volume.
- Outputs that are not a TCP connection (HLS, file recording, pipes, from [M11](m11-sinks-live-dj.md)) restart on a handover: a new segment, file or process. Ogg streams (Opus, Vorbis) continue as a new chained stream.
- Upgrading Icecast itself, MariaDB or the host.

## Deliverables

- `docker/icecast/icecast.xml`, the `icecast-fallback` service and a fallback file in the `MP3 128k` format, in the `icecast` profile of `docker-compose.yml`.
- `heron-engine`: standby mode, the handover protocol (`crates/engine/src/handover.rs`), adopting connections in `heron-outputs`, encoder flush, the role in `/healthz`, no gRPC while in standby. Design: [Engine handover](../handover.md).
- `heron-relay` (`heron-engine relay`): holds the TCP connections to stream servers so they outlive the engine's container; engines reach it through a Unix socket in a shared volume.
- `scripts/upgrade.sh` (Compose and Swarm); `stop_grace_period: 30s` for `heron-engine`.
- An "engine upgrade" case in `scripts/chaos-test.sh`.
- [Upgrading](../upgrading.md), [Audio engine – Upgrades and handover](../audio-engine.md#upgrades-and-handover), [Encoders and outputs – Isolation and reliability](../outputs.md#isolation-and-reliability).

## Acceptance criteria

- [x] With the fallback mount, `docker compose restart heron-engine` keeps a listener connected: it hears the fallback, then the station again, without reconnecting (a source stopped for 5 s: about 4.5 s of fallback, the listener back on `/live` within a second; chaos test step 6).
- [x] `scripts/upgrade.sh` upgrades a running Compose stack to another image tag while a listener is connected: the listener's connection survives, Icecast sees one source connection throughout (no new `PUT`), and the audio gap at the cut is under 200 ms. On amd64 and arm64. (68 ms on arm64, Docker Desktop; chaos test step 7 passing in CI on amd64 and arm64: no gap heard, one source connection, one engine left.)
- [x] The same with Docker Swarm (single node, `start-first`): `docker service update --image …` on `heron-engine` (72 ms, one source connection).
- [x] After a handover, the new engine plays the same queue with the same entry IDs, the track on air within 50 ms of the cut position; the play history has one entry for the track on air, not two; the old engine writes nothing to `queue.json` after the handover. (`tests/handover.rs`: queue, entry IDs, the new engine resuming where the old one is held; `the_old_engine_writes_nothing_after_handing_over`. The history first had two entries when the track moved from deck B to deck A, a bug since M3 on any restart: the API matched the track on air by deck. Fixed, `history::tests`, and checked on a stack: one entry.)
- [x] Failures end as a cold restart, never off air for longer than today: standby killed while preparing, standby never ready (three attempts, then the active engine stops as before), incompatible protocol version, old engine killed after passing its connections (`tests/handover.rs`: the active engine plays on, never held, on its own connection).
- [x] A plain `docker compose pull && docker compose up -d` still upgrades the stack (cold restart, listeners kept by the fallback mount: the engine recreated by `docker compose up -d`, the listener stayed connected, no silence over 0.3 s, the track resumed, one history entry).
- [x] During the overlap the API keeps working, and `WatchEvents` reconnects to the new engine (`/api/v1/player` polled every 0.5 s through an upgrade: 69 answers out of 70, one refused during the switch; the history went on recording).

## References

- [Upgrading](../upgrading.md)
- [Audio engine – Upgrades and handover](../audio-engine.md#upgrades-and-handover)
- [Requirements – Reliability](../requirements.md#5-reliability-247)
