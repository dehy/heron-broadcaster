# Audio engine

How `heron-engine`, the real-time playout process, works. Functional requirements for playout (decks, crossfading, Gap Killer, loudness) are in [Requirements](requirements.md#2-playout). Built in [M2](roadmap/m2-engine-core.md).

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
| Player (tokio task) | Owns the queue, loads tracks, plans transitions, handles the mixer's events every 10 ms | Yes |
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

Decode threads use the analysis' own decoder (`heron_analysis::Decoder`), so the engine plays exactly the samples the analysis measured and cue points line up:

- gapless decoding, with the fallback for MP3s whose broken Xing/LAME header makes gapless trimming drop everything;
- NaN and infinite samples replaced by silence, samples beyond ±4.0 (+12 dB) clamped as corrupt data;
- resampled to 48 kHz (`heron_dsp::StreamResampler`, FFT, aligned with the input and exact in length);
- mono played on both channels; surround reduced to its front left and right;
- cut at the cue points after resampling, so positions in milliseconds mean the same as in the analysis.

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

Each track gets a constant gain bringing its integrated loudness (EBU R128, from the analysis) to the target. Boosts stop at `max_gain_db`, and never push the track's true peak above `peak_ceiling_dbtp`: there is no limiter before [M9](roadmap/m9-agc-multiband.md). Tracks without a measured loudness play at their own level. The master bus is hard-clipped at full scale as a last safety.

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

The crossfade point itself (dB trigger, minimum and maximum fade time) is found at import, see [Import and analysis](analysis.md#crossfade-point).

## Control interface

gRPC service `heron.engine.v1.Engine` ([engine.proto](../crates/proto/proto/heron/engine/v1/engine.proto)), on `HERON_GRPC_ADDR` (port 50051, inside the Docker network only): queue (enqueue, remove, clear), skip, pause, resume, FX player, state, levels, a stream of track changes, playout settings and outputs. Tracks carry everything needed to play them: library path, tags, cue points, crossfade point, loudness and overrides.

The engine keeps its playout settings and outputs in `HERON_STATE_DIR/config.json`, so it restarts streaming even when the API and the database are down. The queue lives in memory; keeping it across restarts comes with [M3](roadmap/m3-reliability.md).

`heron-engine ctl` is a command-line client:

```sh
docker compose exec heron-engine heron-engine ctl state
docker compose exec heron-engine heron-engine ctl enqueue "Artist/Album/01 Track.mp3"
docker compose exec heron-engine heron-engine ctl skip
docker compose exec -T heron-engine heron-engine ctl set-outputs /dev/stdin < outputs.json
```

Until the queue manager arrives with [M4](roadmap/m4-api-control-plane.md), `ctl enqueue` and `ctl autofill` (keeps the queue from running dry with random library tracks) analyze files themselves to send their cue points.

## Signal quality

- Gain changes are ramps (fades, skip, pause), never steps, to avoid clicks.
- Parameter changes of later DSP (EQ, AGC) are **smoothed** too.

## Failure handling

Every failure path ends with audio continuing, in this order:

1. next track;
2. fallback directory;
3. generated silence filler, as the very last resort (with an alert).

In M2, a queued file that cannot be decoded is skipped before it starts; a file that fails mid-way ends there and the next track starts. When the queue runs dry, the engine streams silence. The fallback directory, the silence watchdog and the rest come with [M3 – 24/7 reliability](roadmap/m3-reliability.md). See [Requirements – Reliability](requirements.md#5-reliability-247).

## Observability

`/healthz` fails when the audio thread stops rendering. `/metrics`:

| Metric | Meaning |
|---|---|
| `heron_engine_rendered_seconds` | Audio rendered since the start |
| `heron_engine_clock_drift_seconds` | Audio rendered minus time elapsed: constant (the 50 ms lead) while the clock keeps time |
| `heron_engine_clock_max_late_seconds` | Worst lateness of a block over the last second |
| `heron_engine_clock_resets_total` | Stalls longer than 1 s |
| `heron_engine_underruns_total` | Blocks where a deck had no decoded audio |
| `heron_engine_dropped_blocks_total` | Blocks the encoders did not take in time |
| `heron_output_connected`, `heron_output_bytes_total`, `heron_output_reconnects_total`, `heron_output_dropped_chunks_total` | Per output |
| `heron_encoder_dropped_chunks_total` | Per encoder profile |

## Testability

- Crossfades are rendered offline by the same mixer code and compared, sample by sample, with references computed independently (`crates/engine/src/mixer/tests.rs`).
- The player is tested against real files with the mixer rendered offline.
- `scripts/soak-test.sh` plays the library to two Icecast servers for a given time and checks the clock kept time and nothing was lost; `scripts/check-streaming.sh` checks a Compose stack streams (run in CI on amd64 and arm64).
