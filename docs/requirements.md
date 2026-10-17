# Requirements

Rotation rules and clockwheels have their own document: [Rotation](rotation.md). They are the heart of a good programme: clock construction, how slots call their categories, restrictions, separation between plays of a title or an artist, and artists known under several names.

## 1. Library and categories

- Scan a mounted music library, watch it for changes, read tags.
- Tracks can belong to **several categories** (many-to-many).
- Per-track overrides: cue-in, cue-out, crossfade settings, Gap Killer on/off.

Delivered in [M1 – Import & analysis](roadmap/m1-import-analysis.md) and [M6 – Rotation & clockwheels](roadmap/m6-rotation-clockwheels.md).

## 2. Playout

- **Dual deck** (Deck A / Deck B) plus an FX/jingle player, mixed on a master bus.

### Crossfading

Modeled on SAM Broadcaster's "Cross-Fading" screen.

- Fade-out and fade-in, each with its own:
  - enable flag,
  - curve (linear, S-curve, logarithmic, exponential),
  - time (ms),
  - level (%).
- Crossfade point:
  - **fixed** time before end, or
  - **auto-detected** from a dB trigger (default −15 dB) with min (150 ms) and max (2 500 ms) fade time.
- Do not crossfade tracks of N seconds or less (default 20).
- Because analysis happens at import, crossfades are deterministic and can be previewed in the UI.

| Setting | Default |
|---|---|
| Auto-detect dB trigger | −15 dB |
| Minimum fade time | 150 ms |
| Maximum fade time | 2 500 ms |
| No crossfade for tracks ≤ | 20 s |

### Gap Killer

Modeled on SAM Broadcaster's "Gap Killer" screen, applied from analysis data.

- Remove silence at the start and end of tracks.
- Detect and skip blips of 10 ms or less.
- End the track if more than N seconds of silence is detected.
- Enabled by default, overridable per track.
- Thresholds are measured from the track's integrated loudness, and silence must also be below an absolute ceiling (−50 dBFS), so soft intros are never trimmed. See [Import and analysis](analysis.md#what-counts-as-silence).

| Setting | Default |
|---|---|
| Start silence threshold | −20 dB |
| End silence threshold | −24 dB |
| Blip length | ≤ 10 ms |
| End track after internal silence of | 15 s |

### Loudness

- **Loudness normalization** from the analyzed integrated loudness (EBU R128).

Delivered in [M2 – Engine core](roadmap/m2-engine-core.md). The real-time rules for the engine are in [Audio engine](audio-engine.md).

## 3. Audio processing

### Deck EQ

- 10 bands per deck at 60, 170, 310, 600, 1k, 3k, 6k, 12k, 14k, 16k Hz.
- ±12 dB per band.
- Presets, enable/bypass.

Delivered in [M8 – Deck EQ](roadmap/m8-deck-eq.md).

### Master AGC chain

Modeled on SAM Broadcaster's AGC tab, in this order:

1. **Gated AGC** (gate, max gain)
2. **Stereo expander** (level, depth, threshold)
3. **Bass EQ** (gain, frequency, shelf or peak)
4. **5-band processor** (overdrive, per-band ratio, threshold, attack, release, hold, band gain, links between adjacent bands; compressor / expander / limiter modes)
5. **Dual-band processor** (same controls, LF/HF)
6. **Clipper / final limiter**

Also required: presets, "bypass all", in/out meters and per-band meters in the UI.

Out of scope: pre-emphasis (50/75 µs). It only matters for FM transmitters.

Delivered in [M9 – AGC & multiband processing](roadmap/m9-agc-multiband.md).

## 4. Outputs and integrations

- **Multiple outputs** to different stream servers at the same time, each with its own encoder settings.
- The user **selects the encoder** for each output: MP3 (LAME), AAC-LC (built in), Opus, Vorbis, or an external FFmpeg binary provided by the user (needed for fdk-aac / HE-AAC).
- `OutputSink` trait receiving encoded audio frames and metadata.
  - Icecast source client first.
  - Later: Shoutcast v1/v2, **HLS**, file recording, pipe to an external process.
- Metadata (now playing) pushed to the streamer.
- Details in [Encoders and outputs](outputs.md).
- **Webhooks** on track start, end and skip: configurable payload, HMAC signature, retries with backoff, delivery log.
- REST API, WebSocket for live state, public now-playing and history endpoints.
- **Listener statistics** from the stream servers (Icecast, Shoutcast): listeners per day, per hour, per output and per track, peak, average and total listening hours, without storing IP addresses. See [M7b](roadmap/m7b-listener-statistics.md).

Delivered across [M2](roadmap/m2-engine-core.md), [M4](roadmap/m4-api-control-plane.md), [M7](roadmap/m7-webhooks-output-api.md), [M7b](roadmap/m7b-listener-statistics.md) and [M10](roadmap/m10-sinks-live-dj.md).

## 5. Reliability (24/7)

- The engine keeps the next N tracks decoded or ready in memory, so a database or API outage does not stop the air.
- Local **fallback directory** used when the queue runs dry or the control plane is unreachable.
- **Silence watchdog** on the output: skip to the next track, then to fallback, after N seconds of silence.
- Missing or corrupt files are skipped without dead air.
- Icecast **reconnect** with exponential backoff.
- Engine state persisted locally so a restart resumes the queue.
- Docker restart policies and health checks on every service.
- **Clock sync monitoring:** read the kernel's clock sync status and estimated error (read-only `adjtimex`/`ntp_adjtime`, no `CAP_SYS_TIME` needed) and expose them:
  - in `/metrics`, as `heron_clock_synchronized` and `heron_clock_offset_seconds`;
  - as a warning in the `/healthz` response, and later in the dashboard, when the clock is not synced or is off by more than about 1 s.

  This tells operators when scheduling might be off, without Heron taking over the clock.

Delivered in [M3 – 24/7 reliability](roadmap/m3-reliability.md).
