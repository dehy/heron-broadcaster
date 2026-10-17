# Data model

The schema lives in `migrations/`; this page summarizes it. Tables not used yet are sketches and will change with their milestone. Database: MariaDB through `sqlx`, with migrations in `migrations/` that run automatically on startup.

## Library

| Table | Columns |
|---|---|
| `tracks` | id, path (relative to the library root, `/` separators, unique through a hash), file_hash (fingerprint: size + first and last 64 KiB), file_size, file_mtime_us, title, artist, album, duration_ms, enabled, missing_since, created_at, updated_at |
| `track_analysis` | track_id, analysis_version, params_hash (fingerprint of the analysis settings), integrated_lufs, true_peak_db, cue_in_ms, cue_out_ms, fade_out_start_ms, has_internal_silence, analyzed_at |
| `track_overrides` | track_id, cue_in_ms, cue_out_ms, crossfade settings, gap_killer_enabled (all nullable) |
| `analysis_jobs` | id, track_id, analysis_version, status (pending, running, done, failed), attempts, claimed_by, claimed_at, error, created_at, updated_at — queue between the scanner and `heron-analyzer` (see [Import and analysis](analysis.md#job-queue)) |

## Categories

| Table | Columns |
|---|---|
| `categories` | id, name, color |
| `track_categories` | track_id, category_id, weight |

## Clockwheels

| Table | Columns |
|---|---|
| `clocks` | id, name |
| `clock_slots` | clock_id, position, category_id, selection_method, enforce_rules |
| `clock_schedule` | day_of_week, hour, clock_id |

## History

| Table | Columns |
|---|---|
| `play_history` | id, track_id, started_at, ended_at, deck, source (rotation, request, fallback, manual), relaxation_applied |

## Settings

| Table | Columns |
|---|---|
| `settings` | name, JSON value. `analysis`: Gap Killer and crossfade point detection ([Import and analysis](analysis.md#settings)); `playout`: crossfade and loudness ([Audio engine](audio-engine.md#playout-settings)); later separation rules |

## Encoders and outputs

See [Encoders and outputs](outputs.md). The engine never reads these tables: the API pushes them over gRPC.

| Table | Columns |
|---|---|
| `encoder_profiles` | id, name, encoder (mp3, aac, opus, vorbis, external_ffmpeg), bitrate_kbps, sample_rate, channels, options (JSON: VBR/CBR, ffmpeg binary path and arguments, …). A fresh install has `MP3 128k` |
| `outputs` | id, name, kind (icecast, shoutcast_v1, shoutcast_v2, hls, file, pipe), encoder_profile_id, config (JSON: host, port, mount, credentials, HLS directory and segment settings, …), metadata_enabled, enabled |

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
tracks *──* categories        (through track_categories, with weight)
clocks 1──* clock_slots *──1 categories
clock_schedule *──1 clocks
play_history *──1 tracks
webhooks 1──* webhook_deliveries
encoder_profiles 1──* outputs
```
