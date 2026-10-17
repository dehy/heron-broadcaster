# API and control plane

`heron-api` is the station's control plane: the REST API and live WebSocket the web UI uses, the queue manager feeding the engine, the play history, and the configuration it pushes to the engine. Built in [M4](roadmap/m4-api-control-plane.md).

The engine never depends on it: when the API is down, the engine plays what it has queued with the configuration it last received ([Audio engine](audio-engine.md)).

## Basics

- Every route is under `/api/v1`. `heron-web` proxies `/api` to `heron-api`, so the API is at `http://<host>:8000/api/v1`.
- Requests and responses are JSON. Times are Unix time in milliseconds (`*_unix_ms`), durations in milliseconds (`*_ms`).
- Errors are `{"error": "<code>", "message": "…"}`: `invalid_request` (400), `unauthorized` (401), `forbidden` (403), `not_found` (404), `conflict` (409), `invalid` (422), `too_many_requests` (429), `engine_unavailable` (503), `internal` (500).
- **OpenAPI:** the document is generated from the code and served at `/api/v1/openapi.json`; `heron-api openapi` prints it. It is the reference for every route and type below.

## Authentication

Local users only for now ([Decisions](decisions.md#decisions-made-after-the-handoff)).

| Role | Can |
|---|---|
| `viewer` | See everything except users and secrets (output passwords, the AcoustID key are never sent back to anyone) |
| `operator` | Also run the station: queue, skip, pause, FX, track overrides and categories, queue settings |
| `admin` | Also playout, analysis and library settings, encoder profiles, outputs and users |

Create the first admin on the command line (the password is read without echo; from stdin when it is not a terminal):

```sh
docker compose exec heron-api heron-api user add admin --role admin
docker compose exec heron-api heron-api user passwd admin
docker compose exec heron-api heron-api user list
```

Then:

- **Web app:** `POST /api/v1/auth/login` with `{"username", "password"}` sets a session cookie (`heron_session`, `HttpOnly`, `SameSite=Strict`, `Secure` when the proxy reports HTTPS through `X-Forwarded-Proto`). Sessions last 30 days from the last use. Requests that change something with a cookie must also send an `X-Requested-With` header (any value): pages on other sites cannot add it, so they cannot act on your behalf.
- **Scripts:** create an API token (`POST /api/v1/tokens`, shown once) and send `Authorization: Bearer heron_…`. Tokens act as their user, with its role; they can expire and be revoked.
- Passwords (8 characters at least) are hashed with Argon2id, one at a time so sign-ins never take CPU from the engine on a Raspberry Pi. After 5 failed sign-ins a username is locked for a minute. Only hashes of session and API tokens are stored. Changing a password signs the user out everywhere else; disabling a user ends their sessions and stops their tokens. The last enabled admin cannot be demoted, disabled or deleted.
- `GET /api/v1/status`, `/api/v1/library/status` and `/api/v1/openapi.json` need no sign-in.

## Routes

| Area | Routes |
|---|---|
| Account | `POST /auth/login`, `POST /auth/logout`, `GET /auth/me`, `PUT /auth/password`; `GET`, `POST /tokens`, `DELETE /tokens/{id}` |
| Users (admins) | `GET`, `POST /users`; `PATCH`, `DELETE /users/{id}` |
| Engine and decks | `GET /engine` (reachable, configuration in sync), `GET /player` (decks, cued track, queue, outputs), `GET /player/levels`, `POST /player/skip`, `/player/pause`, `/player/resume`, `POST /player/fx` (`{"track_id"}`), `DELETE /player/fx` |
| Queue | `GET /queue` (the cued track first), `POST /queue` (`{"track_ids", "position"}`), `DELETE /queue`, `DELETE /queue/{entry_id}`, `PUT /queue/{entry_id}/position` |
| Library | `GET /library/status`; `GET /tracks` (`q`, `category_id`, `filter`: playable, missing, unanalyzed, silent, disabled, needs_review; `sort`; `limit`, `offset`); `GET`, `PATCH /tracks/{id}` (`enabled`); `PUT`, `DELETE /tracks/{id}/overrides`; `PUT /tracks/{id}/categories`; `POST /playout/transition` (preview a transition between two tracks, optionally with unsaved settings) |
| Categories | `GET`, `POST /categories`; `GET`, `PUT`, `DELETE /categories/{id}` |
| History | `GET /history` (`before`, `after`, `track_id`, `exclude_fx`, `limit`) |
| Settings | `GET`, `PUT /settings/playout`, `/settings/analysis`, `/settings/queue`; `GET`, `PATCH /settings/library` |
| Outputs | `GET`, `POST /encoder-profiles`; `GET`, `PUT`, `DELETE /encoder-profiles/{id}`; `GET`, `POST /outputs`; `GET`, `PUT`, `DELETE /outputs/{id}` (with each output's live state) |
| Live | `GET /live` (WebSocket) |

Every engine action is there: queue (enqueue, remove, clear, move, and replace, which the queue manager uses), transport (skip, pause, resume), FX player, state, levels, track changes (WebSocket), playout settings and outputs.

## Live WebSocket

`/api/v1/live`, signed in like any request (the browser sends the session cookie). Each message is a `LiveMessage` (see the OpenAPI document), told apart by `type`:

| `type` | When | Content |
|---|---|---|
| `state` | On connection, then whenever the decks, the queue, the outputs or the engine's reachability change | `player`: the same as `GET /player`, `null` while the engine is unreachable |
| `positions` | Twice a second | Each deck's position and length |
| `levels` | Ten times a second, unless the URL has `?levels=false` | Peak and RMS (dBFS) of the master bus and each deck |
| `track` | Each track change | `event`: `started` or `ended` (with `played_ms` and `reason`: finished, skipped, stopped, replaced) |

The API polls the engine's state twice a second and its levels ten times a second, only while someone watches them.

## Queue manager

The engine holds the queue; the API feeds it and edits it.

- Tracks are sent with everything the engine needs, from the database: path, tags, cue points and crossfade point from the analysis, loudness, and the per-track overrides (cue points, Gap Killer, crossfade). Tracks whose file is missing, or analyzed with nothing audible, are refused. Tracks not analyzed yet play whole.
- **Autofill** (`queue` settings, on by default): when fewer than `min_queued` tracks wait (3), it queues enabled, analyzed, audible tracks, never played first, then the least recently played, never one already waiting or on air. It stands in for the rotation until [M6](roadmap/m6-rotation-clockwheels.md). Turned off, the station plays silence once the queue is empty.
- **Positions** count the waiting tracks: 0 is next (before the cued track), 1 right after it, and so on.
- **Following the library:** every 5 seconds, waiting tracks are checked against the database. A track the librarian moved, or whose cue points changed, is updated in place (same entry and position); a track whose file disappeared or was deleted leaves the queue. The track on air plays on (its file is already open).
- Each track carries its source (`rotation` for autofill, `manual` when queued by hand), recorded in the history.

## Play history

Recorded in `play_history` from the engine's track changes: a row when a track starts (track, artist, title, deck, source), completed when it ends (`played_ms`, `end_reason`). FX plays are recorded too (deck `fx`).

While the API is down or cannot reach the engine, the engine plays on and those plays are not recorded. When the API reconnects, the tracks still playing are matched with their rows (or recorded, starting when their position says), and the other open rows are closed at that time, without an end reason.

## Engine configuration

Playout settings, encoder profiles and outputs are stored in MariaDB and edited through the API. The API pushes them to the engine (`SetPlayoutSettings`, `SetOutputs`) when they change, when the engine starts or becomes reachable again, and checks once a minute; `GET /engine` says whether the engine runs the stored configuration (`synced`, `pending` while it is unreachable, `failed` with the engine's error). Encoder profiles and outputs are checked as a whole configuration before they are saved, the way the engine does: an invalid change saves nothing.

Output settings never show their password; an update that leaves it out keeps the stored one.

**Upgrading from M2:** the first time the API reaches the engine, it adopts the engine's configuration instead of replacing it: playout settings never saved through the API, and outputs when the database has none, are copied from the engine with their IDs, so the station stays on air. From then on the database is the reference, and changes made with `heron-engine ctl set-outputs` or `set-settings` are replaced within a minute: use the API.

## Web client

`web/openapi.json` is the OpenAPI document, and `web/src/api/schema.ts` the TypeScript types generated from it; `web/src/api/index.ts` is a typed client (`openapi-fetch`). After changing the API:

```sh
cargo run -p heron-api -- openapi > web/openapi.json
cd web && npm run generate-api
```

CI fails when either file is out of date. The generator (`openapi-typescript`) lives in `web/tools/openapi` with its own lockfile: it needs the TypeScript 5 compiler API, which the app's TypeScript 7 no longer has.

## Metrics

On top of the scanner's: `heron_api_engine_connected` (0 or 1), `heron_api_engine_disconnections_total`, `heron_api_autofill_tracks_total`, `heron_api_logins_total{result}` and `heron_api_live_clients`.

## Limitations

- Autofill is not a rotation: no separation between artists or titles, no categories or clocks before [M6](roadmap/m6-rotation-clockwheels.md).
- Plays made while the API is away are not recorded (the engine keeps no event history).
- The API reads the library: the review of identifications and reorganization (`heron-librarian review`, `organize`, `undo`) are not in the API yet; they come with their screens in [M5](roadmap/m5-web-ui.md).
- HTTPS is left to a reverse proxy in front of `heron-web`.
