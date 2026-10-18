# Web UI

`heron-web` serves the web app (React, Vite, TypeScript) and proxies `/api` to `heron-api`, WebSocket included: open `http://<host>:8000/` and sign in. Built in [M5](roadmap/m5-web-ui.md); the programming pages came with [M6](roadmap/m6-rotation-clockwheels.md). Everything it does goes through the [API](api.md), so scripts can do the same.

## Pages

| Page | What it does | Changes need |
|---|---|---|
| **On air** | Decks A and B (track, progress, remaining time, which is on air), master and deck levels, skip, pause and resume, the FX player (play a jingle over the music), the next tracks, outputs and their state, the engine's state | operator |
| **Queue** | The waiting tracks, the cued one first, with their expected start times; drag to reorder (or use the keyboard: focus the handle, space, arrows, space), play next, remove, clear; add tracks from the library; autofill on or off | operator |
| **History** | Every play by day, with how it ended (played, skipped…) and for how long | |
| **Tracks** | The library: search, filters (playable, not analyzed, silent, missing, disabled, needs review), category, sort, pages; queue one track or a selection | operator |
| **Track** | Waveform with the cue-in, cue-out, where the next track starts and the fades; listen from anywhere, from the cue-in or the ending; drag the points or type them; Gap Killer and crossfade overrides; categories and weights; analysis, file and identification; recent plays | operator |
| **Artists** | Artists and their names (search any name); rename, move a name to another artist or split it off, merge artists, mark artists similar, restrict an artist; the suggestions to review, and the MusicBrainz relationships lookup | operator |
| **Clocks** | Each clock's slots, reordered by dragging (or with the keyboard), with their category, selection method, enforce-rules flag and separation overrides; whether the clock loops across hours; the hour (or the loop) drawn as it will sound, categories running short, an hour simulated | operator |
| **Schedule** | The week, hour by hour: pick a clock and click or drag over the hours, in station time | operator |
| **Categories** | Create, rename, recolor and delete categories; the separation windows they override | operator |
| **Restrictions** | Dayparting, date ranges and sequences on tracks, artists and categories; turn them on and off | operator |
| **Selection log** | Each track rotation picked: clock and slot, rules held or relaxed, and the details (windows, tracks held back, candidates); a simulation of the next day or week, checked rule by rule | operator (simulations) |
| **Review & organize** | Identifications waiting for review (apply a candidate release, or keep the file's tags); organize the library after a dry run; every batch of file changes, with undo | operator (review), admin (organize, undo) |
| **Crossfade & loudness** | Station crossfade and loudness settings, with a live preview of the transition between two tracks (the one on air and the next by default) as you change them | admin |
| **Outputs** | Outputs (Icecast for now) and encoder profiles (MP3 for now), with each output's live state: connected, reconnecting and why, bytes sent | admin |
| **Settings** | Autofill, rotation (separation windows, fallback category, weights), the station's time zone, library layout and identification (AcoustID key), analysis thresholds | operator (queue, rotation), admin (others) |
| **Users** | Users and roles, passwords, enable or disable | admin |
| **Account** | Theme, password, API tokens | |

Viewers see every page (except users) without the controls. Every page shows a banner when something needs attention: the API, the database or the engine unreachable, live updates disconnected, the engine refusing the configuration, or the **server clock** not synchronized or off by more than about a second (from `GET /api/v1/status`, see [API](api.md#authentication)).

## Live updates

The app keeps one WebSocket to `/api/v1/live` ([API – Live WebSocket](api.md#live-websocket)): the decks, queue and outputs change as soon as the engine reports them, positions move smoothly between the twice-a-second updates, and levels arrive ten times a second. It reconnects by itself (from half a second up to 10 s between tries). Changes are sent with REST calls; the queue shows a drag at once and follows the engine's answer.

## Waveform and previews

The waveform comes with the analysis ([Import and analysis](analysis.md)): one point per 50 ms, on a dB scale so quiet intros and fade-outs show. Tracks analyzed before M5 are analyzed again once to get theirs. Previews play the file itself (`GET /api/v1/tracks/{id}/audio`) in the browser, at its own level, without the station's processing; formats the browser cannot play (some FLAC or ALAC files in Safari) stay silent.

Points dragged on the waveform become the track's overrides once saved. Dragging *Next starts* sets a fixed crossfade for the track (that long before its cue-out). Queued entries of the track follow within seconds.

## Themes

Light and dark themes. *System* (the default) follows the device's setting (`prefers-color-scheme`); *Light* or *Dark* is saved per user by the API (`PUT /api/v1/auth/preferences`) and also remembered by the browser, so the first paint uses it.

## Layout

The layout works from phone width up: below 760 px the navigation moves to a menu, tables drop their secondary columns and cards stack.

## Development

```sh
cd web
npm ci
HERON_API_URL=http://localhost:8000 npm run dev   # against a running station's heron-web
npm test                                          # unit tests (Vitest)
npm run typecheck && npm run build
```

`HERON_API_URL` defaults to `http://localhost:8080`, a `heron-api` run with `cargo run -p heron-api`. The API client is typed from the OpenAPI document ([API – Web client](api.md#web-client)). The app uses TanStack Query for requests and dnd-kit for dragging; routing is a small module on the History API (`src/lib/router.tsx`), since nginx answers every path with `index.html`.

## Limitations

- The FX player and transport act at once; there is no undo for a skip.
- Only Icecast outputs and MP3 profiles have forms: the other kinds arrive with their encoders and sinks in [M10](roadmap/m10-sinks-live-dj.md) (the engine refuses them until then).
- Tags cannot be edited in the browser yet: identification or the files' own tags decide them. Editing them comes with [M5b](roadmap/m5b-track-details-tags.md), with labels and the track details (date added, last played, plays, bitrate).
- The clock editor draws each slot with its category's average length: real tracks vary, so the hour as it sounds drifts from the drawing; simulate it to see real picks.
