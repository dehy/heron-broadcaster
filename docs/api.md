# API and control plane

`heron-api` is the station's control plane: the REST API and live WebSocket the [web UI](web.md) uses, the queue manager feeding the engine, the play history, and the configuration it pushes to the engine. Built in [M4](roadmap/m4-api-control-plane.md), completed for the web UI in [M5](roadmap/m5-web-ui.md); rotation came with [M6](roadmap/m6-rotation-clockwheels.md).

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
| `operator` | Also run the station: queue, skip, pause, FX, track overrides and categories, queue and rotation settings, clocks, the schedule, restrictions, artists, simulations, the review of identifications |
| `admin` | Also playout, analysis, library and station (time zone) settings, organizing the library and undoing batches, encoder profiles, outputs and users |

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
- `GET /api/v1/status`, `/api/v1/library/status` and `/api/v1/openapi.json` need no sign-in. `/status` also reports the server clock's synchronization (`clock`, read from the kernel with a read-only `adjtimex`: containers share the host's clock), with a `warning` when it is not synchronized or may be off by more than a second; the web UI shows it on every page.

## Routes

| Area | Routes |
|---|---|
| Account | `POST /auth/login`, `POST /auth/logout`, `GET /auth/me`, `PUT /auth/password`, `PUT /auth/preferences` (`{"theme", "language"}`, each optional: the theme is system, light or dark; the language is system or a language tag such as `fr`); `GET`, `POST /tokens`, `DELETE /tokens/{id}` |
| Users (admins) | `GET`, `POST /users`; `PATCH`, `DELETE /users/{id}` |
| Engine and decks | `GET /engine` (reachable, configuration in sync), `GET /player` (decks, cued track, queue, outputs), `GET /player/levels`, `POST /player/skip`, `/player/pause`, `/player/resume`, `POST /player/fx` (`{"track_id"}`), `DELETE /player/fx` |
| Queue | `GET /queue` (the cued track first), `POST /queue` (`{"track_ids", "position"}`), `DELETE /queue`, `DELETE /queue/{entry_id}`, `PUT /queue/{entry_id}/position` |
| Library | `GET /library/status`; `GET /tracks` (`q`, `category_id`, `filter`: playable, missing, unanalyzed, silent, disabled, needs_review; `labels_all`, `labels_any`, `labels_none` (comma-separated IDs); `never_played`, `not_played_days`, `added_days`, `bitrate_below`, `era`, `mood`, `sound_code`, `genre`, `year_min`, `year_max`, `bpm_min`, `bpm_max`; `sort`: artist, title, album, genre, added, last_played, play_count, bitrate, duration, year, bpm, mood, sound_code, and `desc`; `limit`, `offset`); `GET`, `PATCH /tracks/{id}` (`enabled`); `PUT`, `DELETE /tracks/{id}/overrides`; `PUT /tracks/{id}/categories`; `GET /tracks/{id}/waveform`; `GET /tracks/{id}/audio` (the file, with range requests, for previews); `POST /playout/transition` (preview a transition between two tracks, optionally with unsaved settings) |
| Library management | `POST /library/musicbrainz-details` (`{"all"}`); `GET /library/review`; `POST /tracks/{id}/review/apply` (`{"release_id", "recording_id"}`, one of its candidates), `POST /tracks/{id}/review/keep`; `POST /library/organize` (dry run), `POST /library/organize/{plan_task_id}/apply`; `GET /library/batches`, `POST /library/batches/{id}/undo`; `GET /library/tasks`, `GET /library/tasks/{id}` — see [below](#library-management) |
| Categories | `GET`, `POST /categories`; `GET`, `PUT`, `DELETE /categories/{id}` (with the separation windows a category overrides) |
| Labels | `GET`, `POST /labels`; `PUT`, `DELETE /labels/{id}`; `POST /labels/{id}/merge` (`{"into_id"}`) |
| Selections | `POST /tracks/labels` (`add`, `remove`), `POST /tracks/categories` (`add` with weights, `remove`), `POST /tracks/programming` (`mood`, `sound_code`, `bpm`; `null` clears), `POST /tracks/tags` (a library task) — see [below](#actions-on-a-selection) |
| Artists | `GET /artists` (`q` matches any name, `limit`, `offset`); `GET`, `PATCH /artists/{id}` (`name`, `musicbrainz_id`); `POST /artists/{id}/merge` (`{"artist_id"}`, merged into this one); `PUT /artist-names/{id}` (`{"artist_id"}` moves the name, `{}` splits it off); `PUT`, `DELETE /artists/{id}/similar/{other}`; `GET /artist-suggestions`, `POST /artist-suggestions/{id}` (`{"accept"}`); `POST /artist-suggestions/musicbrainz` (a library task) |
| Rotation | `GET`, `POST /clocks`; `GET`, `PUT`, `DELETE /clocks/{id}` (name, `looping` and slots, replaced as a whole); `POST /clocks/preview` (`{"slots", "looping"}`, saved or not: expected starts, categories running short); `GET`, `PUT /schedule` (the whole week); `GET /restrictions` (`target`, `target_id`), `POST /restrictions`, `PUT`, `DELETE /restrictions/{id}`; `GET /rotation/picks` (the selection log: `before`, `track_id`, `limit`); `POST /rotation/simulate` (`hours`, `start_unix_ms`, `seed`, `clock_id`) |
| History | `GET /history` (`before`, `after`, `track_id`, `exclude_fx`, `limit`) |
| Settings | `GET`, `PUT /settings/playout`, `/settings/analysis`, `/settings/queue`, `/settings/rotation`, `/settings/station`, `/settings/programming`; `GET`, `PATCH /settings/library` |
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
- **Autofill** (`queue` settings, on by default): when fewer than `min_queued` tracks wait (3), it queues the tracks [rotation](#rotation) picks among the enabled, analyzed, audible tracks. Turned off, the station plays silence once the queue is empty.
- **Positions** count the waiting tracks: 0 is next (before the cued track), 1 right after it, and so on.
- **Following the library:** every 5 seconds, waiting tracks are checked against the database. A track the librarian moved, or whose cue points changed, is updated in place (same entry and position); a track whose file disappeared or was deleted leaves the queue. The track on air plays on (its file is already open).
- Each track carries its source (`rotation` for autofill, `manual` when queued by hand), recorded in the history.

## Rotation

Autofill asks `heron-rotation` for each track it adds ([Rotation](rotation.md)):

- The station is loaded from the database each time: playable tracks, their artists and categories, clocks, the schedule, enabled restrictions, similar artists, the `rotation` and `station` settings.
- The history is what played on decks A and B as far back as separation looks, and when each track last played; the waiting tracks are added at their **planned start**: now, plus what the decks still have to play, plus the waiting tracks' lengths (crossfades not counted).
- The clock carries on from the last pick made from a clock. Each pick goes to the **selection log** with its explanation and the engine's queue entry; when the entry starts, its play is linked to the pick and carries its relaxation step. Relaxed picks are logged at `info`.
- When restrictions leave nothing to pick, autofill waits 30 seconds before trying again (any change to rotation wakes it): the engine's fallback directory plays meanwhile.
- **Simulations** (`POST /rotation/simulate`) run on the same station, from now (or `start_unix_ms`) after the real history, without touching the queue, and are audited: `violations` must be empty, `steps` counts the picks per relaxation step. `clock_id` runs one clock at every hour.

The **artist linker** runs every 10 seconds: tracks whose artist or title tag changed are linked to artists again ([Rotation – Artists with several names](rotation.md#artists-with-several-names)), names made of other artists' names become suggestions, and artists nothing refers to any more are forgotten. `POST /artist-suggestions/musicbrainz` queues an `artist_relations` [library task](#library-management): `heron-librarian` looks up every artist with a MusicBrainz ID (one request a second) and suggests the links between the station's artists.

## Library management

The API cannot write files (its library mount is read-only), so what changes files is queued as a **library task** (`library_tasks`) that `heron-librarian` runs, one at a time, in order. The routes answer `202` with the task; poll `GET /library/tasks/{id}` until its `status` is `done` (with its `plan` or `report`) or `failed` (with its `error`). A task stays `pending` while the librarian is stopped.

- **Review:** `GET /library/review` lists the tracks whose identification was not confident enough, with the releases AcoustID found (`candidates`). Applying one queues a `review_apply` task: the librarian looks the release up on MusicBrainz, writes its tags into the file, files it where the layout says and saves the cover, as a `review` batch. Keeping the file's own tags only changes the database (status `kept`).
- **Organize:** `POST /library/organize` queues a dry run; its `plan` lists every move. `POST /library/organize/{plan_task_id}/apply` carries it out as one batch (once per dry run; tracks moved since are skipped).
- **Undo:** `POST /library/batches/{id}/undo` reverts any batch (import, organization, review, tag edit). Undoing a review puts the track back in review.
- **Tag edits:** `POST /tracks/tags` queues an `edit_tags` task, see [below](#actions-on-a-selection).
- **MusicBrainz details:** `POST /library/musicbrainz-details` queues a `musicbrainz_details` task: the librarian looks up the release of every identified track never looked up (`"all": true`: every one again), one release a second, and saves album, record label, catalog number, ISRC, first release year and genres. Once one has finished, new tracks are looked up in the background.

Tasks running when the librarian stops are marked failed; ask again (an interrupted undo resumes). See [Library management](library.md).

## Actions on a selection

The routes acting on several tracks take a `selection`: `{"track_ids": [...]}`, or `{"filters": {...}}` with the filters of `GET /tracks` (every matching track, on every page; at most 100,000). Operators and admins.

- `POST /tracks/labels` adds and removes labels; `POST /tracks/categories` puts tracks in categories (filling a category from a label: `{"filters": {"labels_any": "3"}}`) or takes them out; `POST /tracks/programming` sets the mood and sound code (values of the scales in the `programming` settings) and the tempo, which replaces the measured one (`null` goes back to it). They change the database only and answer how many tracks were selected and how many changes were made.
- `POST /tracks/tags` writes tags into the files (at most 10,000): `set` gives fields the same value on every track (`title`, `artist`, `album`, `album_artist`, `year`, `genre`, `track`, `disc`, `isrc`, `record_label`, `bpm`, `mood`; `null` removes the field), `from_details` writes each track's own ISRC, record label, BPM or mood. It answers `202` with an `edit_tags` library task; its report counts the files written, those already right, and those skipped (missing, changed since the last scan).

## Track details

`GET /tracks` and `GET /tracks/{id}` give, besides the tags: the date added (when the scanner first found the file), the play count and last played, the codec, bitrate and bitrate mode (`cbr`, `vbr`, `lossless`), the year of first release (MusicBrainz, else the year tag) and its era, the tempo (set by hand, else measured), mood, sound code and labels. A track's page adds its sample rate, channels, size, MusicBrainz details and the measured tempo.

Plays count from `play_history` without the FX player and without plays shorter than `min_play_s` of the **programming settings** (30 s by default, 0 counts every start); a play still going, or whose end was missed, counts once it started that long ago. The same settings hold the eras (decades when empty: `70s`, `80s`, `2000s`; or the station's, by years of first release, the first matching one applies) and the mood and sound code scales (by default an energy from 1 to 5, and soft, medium, hard). Tracks keep their mood and sound code when a scale changes; sorting puts values off the scale last.

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

On top of the scanner's: `heron_api_engine_connected` (0 or 1), `heron_api_engine_disconnections_total`, `heron_api_autofill_tracks_total`, `heron_api_rotation_relaxed_total`, `heron_api_logins_total{result}` and `heron_api_live_clients`.

## Limitations

- Planned starts ignore crossfades, and a skip or a track queued by hand moves the tracks after it: their dayparting and clock slot were decided for the planned time.
- The clock follows its slots in order; the top of the hour is not hit exactly (no hard sync).
- Plays made while the API is away are not recorded (the engine keeps no event history).
- HTTPS is left to a reverse proxy in front of `heron-web`.
