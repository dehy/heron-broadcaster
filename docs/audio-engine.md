# Audio engine

How `heron-engine`, the real-time playout process, works. Functional requirements for playout (decks, crossfading, Gap Killer, loudness) are in [Requirements](requirements.md#2-playout). Built in [M2](roadmap/m2-engine-core.md); made to stay on air in [M3](roadmap/m3-reliability.md), and through upgrades in [M8](roadmap/m8-hot-engine-upgrade.md).

## Threads

```
                  gRPC (heron-api, heron-engine ctl)
                                │
                                ▼
 decode thread A ──ring──►┌──────────┐ commands ┌──────────────┐ ring ┌─────────┐ ┌─► encoder thread (MP3 128k) ─┬─► output task (Icecast A)
 decode thread B ──ring──►│  player  │─────────►│ audio thread │─────►│ fan-out │─┤                              └─► output task (Icecast B)
 decode thread FX ─ring──►│ (control)│◄─────────│ mixer, clock │      └─────────┘ └─► encoder thread (MP3 64k) ──► output task (Icecast C)
                          └──────────┘  events  └──────────────┘                 └─► monitor (optional)
```

| Thread | Role | May allocate, lock, log |
|---|---|---|
| **Audio thread** | Renders the master bus 10 ms at a time against the clock: decks, fades, loudness gain, FX player | **No** |
| Decode threads | One per loaded track: decode, resample to 48 kHz, cut at the cue points, buffer 10 s ahead | Yes |
| Preload threads | One per upcoming track: read its file into memory ahead of time | Yes |
| Player (tokio task) | Owns the queue, loads tracks, plans transitions, handles the mixer's events every 10 ms, runs the silence watchdog, saves the queue | Yes |
| Fallback scanner | Scans the fallback directory every minute and analyzes new files | Yes |
| Queue writer | Writes the saved queue to disk | Yes |
| Watchdog | Exits the process when the audio thread is stuck for 10 s, so Docker restarts it | Yes |
| Fan-out | Copies the master bus to every encoder (and the monitor) | Yes |
| Encoder threads | One per encoder profile in use | Yes |
| Output tasks | One per output: connect, stream, metadata, reconnect | Yes |

Every hop between them is bounded and **drops rather than blocks**, so nothing downstream can stall the audio thread, and one encoder or output cannot stall another.

## Format and clock

- Internal format: **f32, stereo, 48 kHz**, blocks of 480 frames (10 ms).
- **No sound card required.** The audio thread renders each block against the system's monotonic clock, 50 ms ahead of real time. Deadlines come from the number of frames rendered since the start, so timing errors never accumulate: 24 h of stream hold exactly 24 h of audio. After a stall of more than 1 s (machine suspended), the clock starts again from now instead of bursting audio, and counts a reset.
- A local monitor output is optional: build with `--features monitor` and run with `--monitor` (`HERON_MONITOR=true`). It plays the master bus on the default sound card (`cpal`), fed from the fan-out like an encoder. Docker images are built without it.

## Real-time safety

- The audio thread **never allocates, locks, does I/O or logs.** It talks to the rest of the engine through lock-free ring buffers (`rtrb`: commands in, events and finished tracks out) and atomics (positions, levels, counters). Finished tracks are handed back to the player, which drops them.
- Decode ahead into buffers. The mixer only reads audio that is ready; a cued track starts only once 100 ms of it is decoded.
- **The engine never touches the database** ([Locked decisions](decisions.md)). It receives tracks with their cue points over gRPC.
- `tests/no_alloc.rs` drives the mixer through crossfades, skips, pauses, the FX player, underruns and queue changes under an allocator that counts every allocation and free made on the audio thread: there must be none.

## Decoding

### Preloading

A track is decoded only 10 s ahead of its deck: without more, a library that becomes slow or unreachable (a network share) would stop the track on air within seconds. So the files of the next queued tracks (3 by default, `HERON_PRELOAD_TRACKS`) are read whole into memory, each by a thread of its own, long before they are cued. The track then decodes from memory and plays to the end even if its file is deleted or the library disappears. A file not read in time, or larger than 512 MiB, is read from disk as it plays; a file that cannot be read at all is skipped when its turn comes.

Decode threads use the analysis' own decoder (`heron_analysis::Decoder`), so the engine plays exactly the samples the analysis measured and cue points line up:

- gapless decoding, with the fallback for MP3s whose broken Xing/LAME header makes gapless trimming drop everything;
- NaN and infinite samples replaced by silence, samples beyond ±4.0 (+12 dB) clamped as corrupt data;
- resampled to 48 kHz (`heron_dsp::StreamResampler`, FFT, aligned with the input and exact in length);
- mono played on both channels; surround reduced to its front left and right;
- cut at the cue points after resampling, so positions in milliseconds mean the same as in the analysis;
- starting more than 30 s into the file (resuming after a restart), the audio before is decoded without being resampled, to get there in a fraction of the time, and resampling starts 1 s before on a frame that falls exactly on an output frame: the audio is the same as from the start.

## Transitions

The player plans each transition when it loads the next track, as soon as the current one starts ([`heron_core::playout::plan_transition`](../crates/core/src/playout.rs)). The plan is a pure function of the analysis, the per-track overrides and the settings, so it is deterministic and the UI can preview it. The audio thread then starts the next track on the exact frame planned.

1. **Where the next track starts:** at the analyzed crossfade point (`fade_out_start_ms`, mode `auto`), or a fixed time before the cue-out (mode `fixed`).
2. **Back to back instead**, the next track starting on the frame the current one ends, when either track is `min_track_ms` long or shorter (20 s: jingles, IDs), when the track disables crossfading, or when there is no usable crossfade point.
3. **Fade-out** of the outgoing track from the moment the next starts, over `fade_out.time_ms` at most (never past the cue-out), down to `fade_out.level_pct`.
4. **Fade-in** of the incoming track over `fade_in.time_ms`, from `fade_in.level_pct`.
5. If the audio of a track ends before its cue-out (truncated file), the next track starts on that frame.

**Skip** fades the current track out over `skip_fade_ms` and starts the next one at once. **Pause** fades out over 10 ms and holds the decks; the stream keeps going with silence. Back-to-back tracks alternate between Deck A and Deck B. The **FX player** plays on top of the decks and does not change what is on air.

Fade curves (shape of a fade-in; a fade-out is its mirror):

| Curve | Shape |
|---|---|
| `linear` | Straight line |
| `s_curve` | Raised cosine, gentle at both ends |
| `logarithmic` | Rises fast, then levels off (a fade-out holds, then drops) |
| `exponential` | Rises slowly, then fast (a fade-out drops, then trails off) |

### Loudness

Each track gets a constant gain bringing its integrated loudness (EBU R128, from the analysis) to the target. Boosts stop at `max_gain_db`, and never push the track's true peak above `peak_ceiling_dbtp`: there is no limiter before [M10](roadmap/m10-agc-multiband.md). Tracks without a measured loudness play at their own level. The master bus is hard-clipped at full scale as a last safety.

## Playout settings

Stored under `playout` in the `settings` table (JSON, missing fields take the defaults), pushed to the engine over gRPC. Per-track crossfade overrides (`track_overrides.crossfade`) use the same fields for one track: fade-out, mode and fixed time for the transition out of it, fade-in for the transition into it, and `"crossfade": false` to play it back to back.

| Setting | Default |
|---|---|
| `crossfade.fade_out` | enabled, `linear`, 2,500 ms, to 0 % |
| `crossfade.fade_in` | disabled (`linear`, 1,000 ms, from 0 %) |
| `crossfade.mode` | `auto` (`fixed`: `fixed_ms` before the cue-out, 2,000 ms) |
| `crossfade.min_track_ms` | 20,000: shorter tracks are never crossfaded |
| `crossfade.skip_fade_ms` | 1,000 |
| `loudness.enabled` | true |
| `loudness.target_lufs` | −16 |
| `loudness.max_gain_db` | 12 |
| `loudness.peak_ceiling_dbtp` | −1 |
| `silence_watchdog.enabled` | true |
| `silence_watchdog.timeout_ms` | 20,000: longer than the analysis' longest silence inside a track (15 s) |

The crossfade point itself (dB trigger, minimum and maximum fade time) is found at import, see [Import and analysis](analysis.md#crossfade-point).

## Control interface

gRPC service `heron.engine.v1.Engine` ([engine.proto](../crates/proto/proto/heron/engine/v1/engine.proto)), on `HERON_GRPC_ADDR` (port 50051, inside the Docker network only): queue (enqueue, remove, clear, move, replace), skip, pause, resume, FX player, state, levels, a stream of track changes, playout settings and outputs. Tracks carry everything needed to play them: library path, tags, cue points, crossfade point, loudness, overrides, and the reason they play (rotation, manual…), carried back in the track changes for the play history. Queue positions count the waiting tracks: 0 is before the cued track.

The engine keeps its state in `HERON_STATE_DIR`, so it restarts streaming even when the API and the database are down:

| File | Contents | Written |
|---|---|---|
| `config.json` | Playout settings and outputs | When the API (or `ctl`) changes them |
| `queue.json` | The track on air and its position, the cued track and the queue, pause, the next entry ID | When the queue changes, every 5 s while playing, and on shutdown |
| `fallback.json` | Analyses of the fallback directory's files | After each analysis |
| `handover.sock` | Unix socket of the active engine, for a new engine to take over ([Upgrades and handover](#upgrades-and-handover)) | Bound while the engine is active |

Files are replaced atomically (written aside, synced, renamed). On start, the engine resumes the track on air where it was (fading in over 500 ms; a track less than 2 s from its end is not resumed), then plays the saved queue. Entry IDs are kept and new ones go on from the saved next ID, so the API's history and queue stay consistent. Tracks from the fallback directory are not saved. A clean stop (`docker compose restart`) saves the exact position; after a crash, the track resumes from the last periodic save, up to 5 s earlier.

`heron-api` feeds the engine and keeps its configuration ([API and control plane](api.md)). `heron-engine ctl` is a command-line client, to inspect the engine or drive it without the API (the soak test does):

```sh
docker compose exec heron-engine heron-engine ctl state
docker compose exec heron-engine heron-engine ctl watch
docker compose exec heron-engine heron-engine ctl skip
```

`ctl enqueue` and `ctl autofill` analyze files themselves to send their cue points (`ctl enqueue --no-analysis` plays a file whole, as the API does with tracks not analyzed yet). With the API running, outputs and settings set with `ctl set-outputs` and `ctl set-settings` are replaced by the API's within a minute.

## Upgrades and handover

An engine replaced the usual way (stop, then start) is off air for a few seconds, and Icecast disconnects every listener when its source leaves. To upgrade without that, a new engine takes over from the running one: the **hot handover**, built in [M8](roadmap/m8-hot-engine-upgrade.md). The procedures that use it, for Docker Compose and Docker Swarm, are in [Upgrading](upgrading.md); its design, in [Engine handover](handover.md).

The running engine (the **active** one) listens on a unix socket in its state directory, `$HERON_STATE_DIR/handover.sock`. Both engines mount the same state volume, so they run on the same host.

### Standby

On start, the engine connects to `handover.sock`:

| Answer | The new engine |
|---|---|
| None (no socket, or nobody listening) | Starts as usual (a **cold start**): resumes `queue.json` and connects its outputs. |
| A hello with a compatible protocol version | Becomes the **standby**: it does not play, opens no output and writes no state file. `/healthz` succeeds with `"role": "standby"`, so Docker and Swarm consider it started. It does not serve gRPC yet: calls from `heron-api` that reach it (during the overlap, the `heron-engine` name points to both engines) fail as unavailable and are retried. |
| A hello with an incompatible version | Waits for the active engine to stop (the socket to disappear), then makes a cold start. |

A standby whose active engine stops without handing over (it crashes, or `handover.sock` closes) makes a cold start.

### The handover

It starts when the active engine receives SIGTERM with a standby connected. Without a standby, the engine stops as before (the queue is saved, then the process exits).

1. **Prepare.** The active engine sends its playout settings, its outputs, the queue (as in `queue.json`) and the **stop**: the position in the track on air where the new engine takes over, about 2 s ahead. The stop never falls in a transition or under an FX jingle, nor less than 1 s before the next transition: the active engine waits for a moment like that first (within the 30 s stop grace period).
2. **Ready.** The standby opens the track on air at the stop (the fast skip used to resume after a restart), queues the waiting tracks and answers `ready` once enough audio is decoded. An answer later than 300 ms before the stop is too late: the active engine prepares again with a stop three times further ahead (6 s, then 18 s), and gives up after three attempts.
3. **Cut.** The active engine stops the track on air on the exact frame of the stop, fading out over the last 10 ms, flushes its encoders to its outputs, then sends the open connections of its outputs (their file descriptors, passed over the socket). The new engine plays from that frame with a 10 ms fade-in, and streams to the same connections, without connecting again. Icecast never sees the source leave; the listeners hear about 80 ms of silence.
4. **Active.** The old engine writes no state any more and exits. The new engine takes `handover.sock` (bound aside, then renamed over it), writes `queue.json`, and serves gRPC; the API's event stream reconnects to it, as after a restart.

The new engine keeps the entry IDs and the position of the track on air, and does not report it as started again, so the play history has one entry for it. Its encoders start fresh: an MP3 stream continues frame after frame, and an Ogg stream (Opus, Vorbis) continues as a new chained stream. Outputs that are not a connection (HLS, file recording, pipes, from [M11](roadmap/m11-sinks-live-dj.md)) are restarted by the new engine. See [Encoders and outputs – Isolation and reliability](outputs.md#isolation-and-reliability).

| Failure during a handover | What happens |
|---|---|
| The standby dies, or is not ready after three attempts | The active engine stops as before; the standby, if alive, makes a cold start once the active engine is gone. |
| The active engine dies before passing its connections | The standby makes a cold start, from the queue saved last (up to 5 s earlier). |
| The active engine dies after passing its connections | The new engine carries on: it has everything it needs. |
| An output's connection is not passed (closed, or not a connection) | The new engine connects that output again, as after a restart. |

## Signal quality

- Gain changes are ramps (fades, skip, pause), never steps, to avoid clicks.
- Parameter changes of later DSP (EQ, AGC) are **smoothed** too.

## Failure handling

Every failure path ends with audio continuing, in this order:

1. next track;
2. fallback directory;
3. generated silence filler, as the very last resort (with an alert).

| Failure | What happens |
|---|---|
| A queued file is missing, unreadable or corrupt | Read ahead into memory when it is queued (it plays even if deleted later); if it cannot be read or decoded, it is skipped before it starts. A file that fails mid-way ends there and the next track starts. |
| The library is slow or unreachable | The next tracks are already in memory ([Preloading](#preloading)). A cued track whose file hangs (not preloaded in time) is given up after 10 s with nothing on air, and a fallback track plays instead; it is not put back in the queue. |
| The queue runs dry (the API is down or cannot reach the engine, autofill is off, restrictions forbid every track) | A track from the **fallback directory** is cued, 30 s before the track on air ends or at once when nothing plays, so it starts with the planned transition. Anything queued replaces a cued fallback track. |
| A track stays silent | The **silence watchdog** skips it after `silence_watchdog.timeout_ms` of silence (below −50 dBFS) on the master bus. If the next track is silent too, it skips to the fallback directory instead. Pauses do not count. |
| Nothing to play at all | Silence is streamed (the outputs stay connected). After 5 s, the engine logs an error, sets `heron_engine_dead_air` and warns in `/healthz`. |
| A stream server is down or restarted | The output reconnects with exponential backoff; the others are not affected. |
| The audio thread is stuck | The process exits after 10 s; Docker restarts it and it resumes its saved queue. |
| The engine restarts | It resumes the track on air and the queue ([Control interface](#control-interface)). Listeners stay connected through an Icecast fallback mount ([Upgrading](upgrading.md#icecast-fallback-mount)). |
| The engine is upgraded | The new engine takes over the queue and the open connections from the old one, without a gap ([Upgrades and handover](#upgrades-and-handover)). |

The fallback directory (`HERON_FALLBACK_DIR`, mounted read-only from `./data/fallback`) holds a few tracks that suit any hour: station IDs and evergreen music. Its files are scanned every minute (sub-folders too) and analyzed once in the background, like library tracks at import, so they play with their cue points, crossfades and loudness; the analyses are kept in `fallback.json`. A file not analyzed yet plays whole at its own level; silent and undecodable files are never picked. Every file plays once, in random order, before any plays again.

The chaos tests (`scripts/chaos-test.sh`) check each of these on a running stack. See [Requirements – Reliability](requirements.md#5-reliability-247).

## Observability

`/healthz` reports the engine's `role` (`active`, or `standby` while waiting to take over), fails when the audio thread of an active engine stops rendering, and lists `warnings` when the server clock is not synchronized (or off by more than about 1 s) or there is nothing to play. `/metrics`:

| Metric | Meaning |
|---|---|
| `heron_engine_rendered_seconds` | Audio rendered since the start |
| `heron_engine_clock_drift_seconds` | Audio rendered minus time elapsed: constant (the 50 ms lead) while the clock keeps time |
| `heron_engine_clock_max_late_seconds` | Worst lateness of a block over the last second |
| `heron_engine_clock_resets_total` | Stalls longer than 1 s |
| `heron_engine_underruns_total` | Blocks where a deck had no decoded audio |
| `heron_engine_dropped_blocks_total` | Blocks the encoders did not take in time |
| `heron_engine_silent_seconds` | How long the master bus has been silent (below −50 dBFS) |
| `heron_engine_dead_air` | 1 when there is nothing to play |
| `heron_engine_fallback_tracks_total`, `heron_engine_silence_skips_total` | Tracks started from the fallback directory, tracks skipped by the silence watchdog |
| `heron_engine_preload_failures_total`, `heron_engine_stalled_tracks_total` | Queued tracks that could not be read ahead of time, cued tracks given up because they could not start |
| `heron_clock_synchronized`, `heron_clock_offset_seconds`, `heron_clock_estimated_error_seconds` | The server clock's synchronization, from the kernel (read-only `adjtimex`) |
| `heron_output_connected`, `heron_output_bytes_total`, `heron_output_reconnects_total`, `heron_output_dropped_chunks_total` | Per output |
| `heron_encoder_dropped_chunks_total` | Per encoder profile |

Dashboards, alerts and the full list: [Monitoring](monitoring.md).

## Testability

- Crossfades are rendered offline by the same mixer code and compared, sample by sample, with references computed independently (`crates/engine/src/mixer/tests.rs`).
- The player is tested against real files with the mixer rendered offline.
- The player is tested with the fallback directory, the silence watchdog, and a restart resuming the saved queue.
- The handover is tested with two engines on one state directory and a fake Icecast server: one source connection throughout, the same queue and position after the cut, and every failure case ending in a cold start.
- `scripts/chaos-test.sh` breaks a running stack (database, API, Icecast, deleted files, a silent file, engine restarts, an engine upgrade through `scripts/upgrade.sh`) while listening to the stream.
- `scripts/soak-test.sh` plays the library to two Icecast servers for a given time and checks the clock kept time and nothing was lost; `scripts/check-streaming.sh` checks a Compose stack goes on air through the API (run in CI on amd64 and arm64).
