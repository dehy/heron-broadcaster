# Changelog

What changed in each release of Heron. Before upgrading, read the release's notes in [Upgrading](docs/upgrading.md#release-notes).

## Unreleased

### Changed

- **Clearer page names**, in all four languages: *Buffer* is now **Import music**, *Import* is **Migrate**, *Review & organize* is **To organize**, *Selection log* is **Rotation log** and *AGC* is **Processing**. Their addresses changed with them (`/import-music`, `/migrate`, `/to-organize`, `/rotation-log`, `/processing`) and the old ones are not redirected: update any bookmark. The API is unchanged.

## 0.2.0 – 2026-11-01

Milestones M9 (deck EQ), M13 (several stations, see [Not yet](#not-yet)) and M14 (several schedules), medleys, category types, imports from another software and the external FFmpeg encoder. Upgrading from 0.1.0 keeps the station on air: see [0.1.0 to 0.2.0](docs/upgrading.md#010-to-020).

### Added

- **Several stations** in one Heron, sharing the library ([Several stations](docs/stations.md)). Each station has its own engine (one more service of the deployment, `HERON_ENGINE_URL_<CODE>`), programming, queue, playout settings, EQ, outputs, history and webhooks. Categories, medleys and restrictions are tagged with the stations they belong to; artist, title and track windows can apply across stations; plays are counted per station; users get a role on each station. A station switcher appears in the web interface once there are two. The single-station API routes and public endpoints answer for the first station.
- **Several schedules per station:** a default schedule, and others that play during their periods (a season, the summer, every year). The schedule editor paints a whole day, an hour of every day, the week or a rectangle, and copies a day.
- **Deck EQ:** a 10-band EQ for the music decks and another for the FX player, ±12 dB, heard as the sliders move, without clicks, with presets for several styles of music ([Audio engine – Deck EQ](docs/audio-engine.md#deck-eq)). Flat and bypassed until it is turned on.
- **Medleys:** the hooks of several songs strung together with the station's liners, between them or over the music, built from a recipe, edited on a timeline (moves, nudged points, crossfades, fade-in and fade-out, where the music goes down and back up around a liner), previewed, then mixed into a track of the library named after its artists or titles and the day ([Medleys](docs/medleys.md)).
- **Category types:** Song, Jingle, Ad and the station's own. A type can never play twice in a row, and can send no metadata to the stream servers (the last song's stays). A jingle can play on the FX player over the next song's intro, ending where its vocals start.
- **Importing from another software:** an export of another radio automation software (titles, artists, categories, fades, intro and hook points) matched to the library by file name, checked in a dry run where conflicts are settled, applied as one import that can be undone ([Importing from another software](docs/import.md)).
- **External FFmpeg encoder:** any codec of the station's own `ffmpeg`, such as AAC+ with `libfdk_aac`, configured from the Outputs page (`HERON_FFMPEG`, [Encoders and outputs](docs/outputs.md#external-ffmpeg-encoder)).
- **`queue.changed` webhook** and a public queue endpoint: the upcoming tracks in airing order, for a website or a display.
- **Tags kept in Heron** when `write_tags` is off: edits are saved in the database and applied over the file's tags; a track can go back to its file's tags.
- **Where the next track starts** is always shown on the track page and set like a cue point, also on a short track; a transition can be tried and listened to from a track.
- **Web interface:** the category of each deck and in *Up next*, a breadcrumb above the page title, a per-user option to use the whole width of the screen, range selection with Shift + click and Ctrl/⌘ + click, enable or disable a selection, a category created from a track, a selection or the buffer, the intro countdown and the playhead time on the decks, imported intro and hook points on the track page, enforce rules turned off from the slots running short.
- **Forks** publish their own images: `HERON_REGISTRY` in `.env` picks where a station pulls them from.

### Fixed

- Imports up to 64 MiB pass through the web container's nginx.
- A track's year shows without a thousands separator.
- Requests sent to a station keep their body, and the web interface opens one live socket per station.

### Not yet

- Several stations (M13) works and is documented, but two of its acceptance criteria are still to be demonstrated: two stations on air at once with a hot handover of one leaving the other untouched, and a medley tagged with two stations landing in both stations' medley categories. `scripts/upgrade.sh` hands over the first station's engine only: the other stations' engines restart cold, covered by Icecast's fallback mount.
- AGC and multiband processing (M10), more encoders, sinks and live DJ (M11), configuration backups (M12).

## 0.1.0 – 2026-10-26

The first release, milestones M0 to M8: library import and analysis, the buffer zone for new music, library management, playout with two decks, crossfades, Gap Killer and loudness normalization to Icecast, 24/7 reliability, upgrades on air by hot handover, rotation with categories, clocks and a weekly schedule, webhooks and public endpoints, listener statistics, and the web interface in English and French.
