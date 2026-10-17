# M4 – API & control plane

**Depends on:** M2

## Scope

- axum REST API: library, categories, queue, decks, settings.
- WebSocket: now playing, deck positions, levels, queue changes.
- Authentication (local users only for now) and roles.
- OpenAPI spec generated from code.
- Queue manager feeding the engine: tracks sent with their analysis and overrides (replaces `heron-engine ctl enqueue` and `autofill`).
- Play history recorded from the engine's track-change events (`play_history`).
- Playout settings, encoder profiles and outputs stored in MariaDB and pushed to the engine (`SetPlayoutSettings`, `SetOutputs`), with the outputs' state.

## Acceptance criteria

- [x] Every engine action is available through the API.

  Queue (enqueue, remove, clear, move; replace, used by the queue manager), skip, pause, resume, FX player, state, levels, track changes (WebSocket), playout settings and outputs: see [API and control plane](../api.md#routes). `crates/api/src/e2e.rs` drives each of them over HTTP and WebSocket against a real engine playing a library in process, with a scratch MariaDB database.
- [x] The OpenAPI spec is published and used to generate the web client types.

  Served at `/api/v1/openapi.json` (and `heron-api openapi`), committed as `web/openapi.json`; `web/src/api/schema.ts` is generated from it and the web app's client is typed by it. CI fails when either is out of date, and a test checks the document has no dangling reference or duplicate operation.

## Results

- Design: [API and control plane](../api.md).
- **Authentication:** users with three roles (viewer, operator, admin), session cookies (HttpOnly, SameSite=Strict, plus an `X-Requested-With` header on changes) and API tokens; Argon2id, hashed one at a time; sign-in lockout. The first admin is created with `heron-api user add`.
- **Queue manager:** builds the engine's tracks from the analysis and overrides; autofill with the least recently played tracks stands in for the rotation until [M6](m6-rotation-clockwheels.md); queued tracks follow the librarian's moves, cue point changes and missing files.
- **Play history** from the engine's track changes, with how each play ended, reconciled after reconnections.
- **Engine configuration** in MariaDB, checked before it is saved and pushed when it changes or the engine reconnects. The first time, the API adopts what the engine runs (outputs set with `ctl` in M2), keeping their IDs, so stations stay on air when upgrading.
- **Engine:** new `MoveQueueEntry` and `ReplaceQueueEntry` calls, a play source on each track, the engine's start time in its state. Fixed: enqueuing at a position other than 0 went one place too far when a track was cued.
- **End to end in Compose:** `scripts/check-streaming.sh` (CI, amd64 and arm64) creates an admin, adds an Icecast output through the API and checks the stream, the autofilled queue and the history.
- Deferred: the review of identifications and library reorganization through the API ([M5](m5-web-ui.md), with their screens); plays made while the API is down are not recorded.

## References

- [API and control plane](../api.md)
- [Architecture – Processes](../architecture.md#processes)
- [Requirements – Outputs and integrations](../requirements.md#4-outputs-and-integrations)
- [Decisions – Authentication](../decisions.md#decisions-made-after-the-handoff)
