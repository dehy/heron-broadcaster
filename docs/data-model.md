# Data model

The schema lives in `migrations/`; this page summarizes it. Tables not used yet are sketches and will change with their milestone. Database: MariaDB through `sqlx`, with migrations in `migrations/` that run automatically on startup.

## Library

| Table | Columns |
|---|---|
| `tracks` | id, path (relative to the library root, `/` separators, unique through a hash), file_hash (content hash: size + first and last 64 KiB), file_size, file_mtime_us, title, artist, album, duration_ms, enabled, missing_since, created_at, updated_at |
| `track_analysis` | track_id, analysis_version, params_hash (fingerprint of the analysis settings), integrated_lufs, true_peak_db, cue_in_ms, cue_out_ms, fade_out_start_ms, has_internal_silence, analyzed_at |
| `track_overrides` | track_id, cue_in_ms, cue_out_ms, crossfade settings, gap_killer_enabled (all nullable) |
| `analysis_jobs` | id, track_id, analysis_version, status (pending, running, done, failed), attempts, claimed_by, claimed_at, error, created_at, updated_at — queue between the scanner and `heron-analyzer` (see [Import and analysis](analysis.md#job-queue)) |
| `track_fingerprints` | track_id, chromaprint (compressed, first two minutes), created_at |
| `track_identifications` | track_id, status (identified, needs_review), reason, score, recording_mbid, release_mbid, candidates (JSON, for the review screen), updated_at |

## Library management

See [Library management](library.md).

| Table | Columns |
|---|---|
| `library_batches` | id, kind (import, organize, undo), undoes (the batch an undo reverts), created_at, finished_at |
| `library_moves` | id, batch_id, track_id, from_root (incoming, library), from_path, to_path, method (rename, copy), temp_path, content_hash (copies), status (pending, moved, done, abandoned, undone), error — the journal, written before a file is touched |
| `track_tag_changes` | id, track_id, move_id, previous and applied tags (JSON), cover_embedded, created_at, undone_at |

## Categories

| Table | Columns |
|---|---|
| `categories` | id, name, color, separation overrides (JSON) |
| `track_categories` | track_id, category_id, weight |

## Artists

See [Rotation – Artists with several names](rotation.md#artists-with-several-names). Sketch, defined in [M6](roadmap/m6-rotation-clockwheels.md).

| Table | Columns |
|---|---|
| `artists` | id, name (display), musicbrainz_id, group_id |
| `artist_names` | artist_id, name, normalized name (unique) |
| `artist_groups` | id, name, counts_as_one_artist |
| `track_artists` | track_id, artist_id, position |

## Restrictions

See [Rotation – Restrictions](rotation.md#restrictions). Sketch, defined in [M6](roadmap/m6-rotation-clockwheels.md).

| Table | Columns |
|---|---|
| `restrictions` | id, target (track, artist, category), target_id, kind (dayparting, date range, sequence), rule (JSON: days and hours, dates, category it may not follow), enabled, note |

## Clockwheels

| Table | Columns |
|---|---|
| `clocks` | id, name |
| `clock_slots` | clock_id, position, category_id, selection_method, enforce_rules, separation overrides (JSON) |
| `clock_schedule` | day_of_week, hour, clock_id |

## History

| Table | Columns |
|---|---|
| `play_history` | id, track_id, started_at, ended_at, deck, source (rotation, request, fallback, manual), relaxation_applied |

## Settings

| Table | Columns |
|---|---|
| `settings` | name, JSON value. `analysis`: Gap Killer and crossfade point detection ([Import and analysis](analysis.md#settings)); `library`: AcoustID key, layout, covers ([Library management](library.md#settings)); `playout`: crossfade and loudness ([Audio engine](audio-engine.md#playout-settings)); later separation rules |

## Encoders and outputs

See [Encoders and outputs](outputs.md). The engine never reads these tables: the API pushes them over gRPC.

| Table | Columns |
|---|---|
| `encoder_profiles` | id, name, encoder (mp3, aac, opus, vorbis, external_ffmpeg), bitrate_kbps, sample_rate, channels, options (JSON: VBR/CBR, ffmpeg binary path and arguments, …). A fresh install has `MP3 128k` |
| `outputs` | id, name, kind (icecast, shoutcast_v1, shoutcast_v2, hls, file, pipe), encoder_profile_id, config (JSON: host, port, mount, credentials, HLS directory and segment settings, …), metadata_enabled, enabled |

## Listener statistics

See [M7b](roadmap/m7b-listener-statistics.md). Sketch, defined with that milestone. No IP address is ever stored.

| Table | Columns |
|---|---|
| `listener_samples` | output_id, sampled_at (each minute), listeners |
| `listener_sessions` | id, output_id, listener_key (daily salted hash of IP and player), user_agent, started_at, ended_at, duration_s |
| `listener_daily` | output_id, day, hour (NULL for the whole day), distinct_listeners, peak, average, listening_hours, sessions |

## Integrations and access

| Table | Columns |
|---|---|
| `webhooks` | to be defined |
| `webhook_deliveries` | to be defined |
| `users` | local accounts: to be defined (username, password hash, role, …) |
| `sessions` or API tokens | to be defined. Authentication is local only for now ([Decisions](decisions.md#decisions-made-after-the-handoff)) |

## Relationships

```
tracks 1──1 track_analysis
tracks 1──0..1 track_overrides
tracks 1──* analysis_jobs
tracks 1──0..1 track_fingerprints, track_identifications
library_batches 1──* library_moves 1──0..1 track_tag_changes *──1 tracks
tracks *──* categories        (through track_categories, with weight)
tracks *──* artists           (through track_artists)
artists 1──* artist_names
artist_groups 1──* artists
restrictions *──1 tracks | artists | categories
clocks 1──* clock_slots *──1 categories
clock_schedule *──1 clocks
play_history *──1 tracks
webhooks 1──* webhook_deliveries
encoder_profiles 1──* outputs
```
