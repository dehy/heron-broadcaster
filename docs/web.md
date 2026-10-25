# Web UI

`heron-web` serves the web app (React, Vite, TypeScript) and proxies `/api` to `heron-api`, WebSocket included: open `http://<host>:8000/` and sign in. Built in [M5](roadmap/m5-web-ui.md); the programming pages came with [M6](roadmap/m6-rotation-clockwheels.md), the translations with [M5d](roadmap/m5d-web-i18n.md). Everything it does goes through the [API](api.md), so scripts can do the same.

## Pages

| Page | What it does | Changes need |
|---|---|---|
| **On air** | Decks A and B (track, progress, remaining time, which is on air), master and deck levels, skip, pause and resume, the FX player (play a jingle, liner or voice track over the music: it opens on the categories for the FX player, the last one used first, with the whole library as a last tab), the next tracks, outputs and their state, the engine's state | operator |
| **Queue** | The waiting tracks, the cued one first, with their expected start times; drag to reorder (or use the keyboard: focus the handle, space, arrows, space), play next, remove, clear; add tracks from the library; autofill on or off | operator |
| **History** | Every play by day, with how it ended (played, skipped…) and for how long | |
| **Tracks** | The library: search, filters (playable, not analyzed, silent, missing, disabled, needs review), category, labels (any, all, none of), never played, not played for N days, added in the last N days, bitrate below, era, mood, sound code, the tracks of one import (from the Buffer page); sort by any column, either way; choose the columns (added, last played, plays, bitrate, year, era, BPM, mood, sound code, genre, labels); select tracks, a page or every matching track, then queue them, label them, put them in a category, set their mood, sound code or BPM, or edit their tags; delete a selection (to the trash, after a confirmation that lists what goes) | operator |
| **Buffer** | Upload files or folders (dragged, several at once, in resumable chunks, with each file's progress); the new files waiting to be imported, by group: tags as read and as they will be written, cover, planned path, identification and candidate releases, flags (duplicates, unreadable, too short); listen; pick a release or keep the file's tags; edit the tags of a file or a selection (title, artist, album, album artist, year, track, disc; edited fields are marked and can be undone); prepare a selection or a group for its tracks (categories, labels, mood, sound code, shown on each file and summed up by group); import or discard a selection, then follow the link to the tracks just imported. Clean files are selected already | operator |
| **Trash** | Deleted tracks, when they are removed for good; restore a deletion; empty the trash | operator (restore), admin (empty) |
| **Track** | Waveform with the cue-in, cue-out, where the next track starts and the fades; listen from anywhere, from the cue-in or the ending; drag the points or type them; Gap Killer and crossfade overrides; categories and weights; labels, era, BPM, mood, sound code and the MusicBrainz details; analysis, file (codec, bitrate, sample rate, channels) and identification; plays; edit its tags; delete it | operator |
| **Artists** | Artists and their names (search any name); rename, move a name to another artist or split it off, merge artists, mark artists similar, restrict an artist; the suggestions to review, and the MusicBrainz relationships lookup | operator |
| **Clocks** | Each clock's slots, reordered by dragging (or with the keyboard), with their category, selection method, enforce-rules flag and separation overrides; whether the clock loops across hours; the hour (or the loop) drawn as it will sound, categories running short, an hour simulated | operator |
| **Schedule** | The week, hour by hour: pick a clock and click or drag over the hours, in station time | operator |
| **Labels** | Create, rename, recolor, merge and delete labels, with their tracks | operator |
| **Categories** | Create, rename, recolor and delete categories; the separation windows they override; whether the FX player offers them (liners, power intros, voice tracks); fill a category from a label | operator |
| **Restrictions** | Dayparting, date ranges and sequences on tracks, artists and categories; turn them on and off | operator |
| **Selection log** | Each track rotation picked: clock and slot, rules held or relaxed, and the details (windows, tracks held back, candidates); a simulation of the next day or week, checked rule by rule | operator (simulations) |
| **Review & organize** | Identifications waiting for review (apply a candidate release, or keep the file's tags); organize the library after a dry run; every batch of file changes (tag edits included), with undo | operator (review), admin (organize, undo) |
| **Crossfade & loudness** | Station crossfade, loudness and silence watchdog settings, with a live preview of the transition between two tracks (the one on air and the next by default) as you change them | admin |
| **Outputs** | Outputs (Icecast for now) and encoder profiles (MP3 for now), with each output's live state: connected, reconnecting and why, bytes sent | admin |
| **Settings** | Autofill, rotation (separation windows, fallback category, weights), the station's time zone, programming (what counts as a play, eras, mood and sound code scales) and the MusicBrainz details lookup, library layout and identification (AcoustID key), upload limits and how long the trash keeps tracks, analysis thresholds | operator (queue, rotation, programming), admin (others) |
| **Webhooks** | Webhooks on track changes: address, events, FX or not, the default JSON body or a template; the secret, shown once (and a new one); a test ping; each webhook's deliveries, with their status, attempts, what was sent and answered, and send again | admin |
| **Users** | Users and roles, passwords, enable or disable | admin |
| **Account** | Theme, language, password, API tokens | |

Viewers see every page (except users and webhooks) without the controls. Every page shows a banner when something needs attention: the API, the database or the engine unreachable, live updates disconnected, the engine refusing the configuration, or the **server clock** not synchronized or off by more than about a second (from `GET /api/v1/status`, see [API](api.md#authentication)).

## Live updates

The app keeps one WebSocket to `/api/v1/live` ([API – Live WebSocket](api.md#live-websocket)): the decks, queue and outputs change as soon as the engine reports them, positions move smoothly between the twice-a-second updates, and levels arrive ten times a second. It reconnects by itself (from half a second up to 10 s between tries). Changes are sent with REST calls; the queue shows a drag at once and follows the engine's answer.

## Waveform and previews

The waveform comes with the analysis ([Import and analysis](analysis.md)): one point per 50 ms, on a dB scale so quiet intros and fade-outs show. Tracks analyzed before M5 are analyzed again once to get theirs. Previews play the file itself (`GET /api/v1/tracks/{id}/audio`) in the browser, at its own level, without the station's processing; formats the browser cannot play (some FLAC or ALAC files in Safari) stay silent.

Points dragged on the waveform become the track's overrides once saved. Dragging *Next starts* sets a fixed crossfade for the track (that long before its cue-out). Queued entries of the track follow within seconds.

## Times

Every time and date is shown in the station's time zone (`station` settings, see [Rotation](rotation.md)), whatever the browser's: the history's days, the queue's start times, the selection log and the schedule agree with the clocks and restrictions. The History and Schedule pages name the zone.

## Themes

Light and dark themes. *System* (the default) follows the device's setting (`prefers-color-scheme`); *Light* or *Dark* is saved per user by the API (`PUT /api/v1/auth/preferences`) and also remembered by the browser, so the first paint uses it.

## Languages

English and French. *System* (the default) follows the browser's languages (`navigator.languages`: the first one the web UI has, English otherwise); a fixed language is saved per user by the API (`PUT /api/v1/auth/preferences`, `{"language": "fr"}`) and remembered by the browser, like the theme, so the sign-in page and the first paint already use it. `<html lang>` follows.

Dates, times, numbers, sizes, levels, relative times and plurals are written the language's way (`-1,5 dB`, *il y a 5 minutes*, *2 pistes*), with the browser's own variant of it when it has one (`en-GB` writes 24-hour times). Duration fields take a comma as well as a dot (`1:23,5`). Only the language changes, never the time zone (see [Times](#times)).

Not translated: what the station's people wrote or the files carry (tags, artist, category, clock and output names), the messages the API writes (the web translates the error codes it knows and shows the others' message as it is), logs, the command-line tools and the documentation.

### Adding a language

1. Copy `web/src/i18n/fr.ts` to `web/src/i18n/<code>.ts` and translate it. Messages are keyed by id; `{name}` is a parameter, which a translation can move but not rename; plurals are objects of [`Intl.PluralRules` forms](https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html) (`one`, `few`, `many`, `other`…) chosen by `{count}`, `other` being required.
2. Add a line to `LANGUAGES` in `web/src/i18n/index.ts`, with the language's name in that language.
3. `npm run typecheck` fails on a missing or misspelled key, and `npm test` (`catalogs.test.ts`) on a parameter that differs from English.
4. Check the pages with the pseudo-locale (below), then in the language at phone width.

Catalogs other than English are loaded only when chosen. The API takes any language tag, so adding one needs no change there.

### Pseudo-locale

In development, `?lang=pseudo` on any page shows English accented, a third longer and in brackets (`[Šàṽé ··]`): text left out of the catalogs stands out, and so do layouts that break with longer words. It lasts until the page is reloaded without it, and is never saved.

## Layout

The layout works from phone width up: below 760 px the navigation moves to a menu, tables drop their secondary columns and cards stack.

## Development

```sh
cd web
npm ci
HERON_API_URL=http://localhost:8000 npm run dev   # against a running station's heron-web
npm test                                          # unit tests (Vitest), catalogs included
npm run typecheck && npm run build
```

Open a page with `?lang=pseudo` to check translations ([Pseudo-locale](#pseudo-locale)). `HERON_API_URL` defaults to `http://localhost:8080`, a `heron-api` run with `cargo run -p heron-api`. The API client is typed from the OpenAPI document ([API – Web client](api.md#web-client)). The app uses TanStack Query for requests and dnd-kit for dragging; routing is a small module on the History API (`src/lib/router.tsx`), since nginx answers every path with `index.html`.

## Limitations

- The FX player and transport act at once; there is no undo for a skip.
- Only Icecast outputs and MP3 profiles have forms: the other kinds arrive with their encoders and sinks in [M10](roadmap/m10-sinks-live-dj.md) (the engine refuses them until then).
- An upload resumes after a dropped connection, but not after the page is reloaded. Covers in the buffer are loaded by the browser from the Cover Art Archive.
- The clock editor draws each slot with its category's average length: real tracks vary, so the hour as it sounds drifts from the drawing; simulate it to see real picks.
