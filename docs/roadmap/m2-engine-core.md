# M2 – Engine core

**Depends on:** M1

## Scope

- Decode and resample to the internal format.
- Deck A / Deck B + FX player, master bus.
- Crossfade engine with all parameters from [Playout](../requirements.md#2-playout):
  - crossfade point from analysis or fixed;
  - no crossfade under N seconds;
  - per-track overrides.
- Apply cue-in / cue-out (Gap Killer) and loudness normalization from analysis.
- Use the same decoding path as the analysis (gapless, with the Xing fallback), so cue points line up, and never let out-of-range decoded samples reach the output (seen on real corrupt MP3s, see [Import and analysis](../analysis.md#analyzer)).
- Real-time pacing clock; optional monitor output.
- Encoder/output model from [Encoders and outputs](../outputs.md): encoder profiles, `OutputSink` trait, encoder fan-out, **multiple outputs at once**, each isolated from the audio thread and from each other.
- MP3 encoder (LAME) and Icecast sink, metadata updates.
- Migration seeding the default `MP3 128k` profile.
- gRPC control: load tracks, skip, pause, read state and levels.

## Acceptance criteria

- [ ] Plays a queue gaplessly with crossfades to Icecast for 24 h without drift or glitches.
- [x] No allocation or locking in the audio thread (code review plus an allocation-detecting allocator in tests).
- [x] Offline-rendered crossfades match reference renders within tolerance.
- [x] Streams simultaneously to at least two Icecast servers with different bitrates; disconnecting one does not affect the other.

## Results

- **Audio thread:** `crates/engine/tests/no_alloc.rs` counts every allocation and free made on the audio thread while the mixer goes through crossfades, skips, pauses, the FX player, underruns, queue changes and decoded tracks: none. It only uses lock-free ring buffers and atomics.
- **Crossfades:** offline renders match references computed independently, sample by sample, within 1e-5, with transitions falling in the middle of blocks (block sizes 1 to 1,000 frames). Back-to-back tracks are sample-exact (gapless).
- **Two Icecast servers:** tested automatically with fake servers (`crates/outputs/tests/two_outputs.rs`) and live with two Icecast 2.4.4 servers at 128 and 64 kbps: with one server stopped, the other kept receiving 8,025 bytes/s (64 kbps), and the stopped one reconnected on its own with backoff when it came back.
- **Listening:** a 75 s capture of the stream across three transitions has no silence (besides the MP3 encoder's 45 ms priming at the start); handovers land on the analyzed crossfade points. CI streams queued tracks to Icecast in Docker Compose on amd64 and arm64 (`scripts/check-streaming.sh`).
- **24 h run:** pending. `scripts/soak-test.sh` runs it and checks drift, underruns, dropped audio, clock resets and reconnects.

## References

- [Audio engine](../audio-engine.md)
- [Encoders and outputs](../outputs.md)
- [Encoders and outputs – Default profiles](../outputs.md#default-profiles)
