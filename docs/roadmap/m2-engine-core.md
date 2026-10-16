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
- Real-time pacing clock; optional monitor output.
- Encoder/output model from [Encoders and outputs](../outputs.md): encoder profiles, `OutputSink` trait, encoder fan-out, **multiple outputs at once**, each isolated from the audio thread and from each other.
- MP3 encoder (LAME) and Icecast sink, metadata updates.
- gRPC control: load tracks, skip, pause, read state and levels.

## Acceptance criteria

- [ ] Plays a queue gaplessly with crossfades to Icecast for 24 h without drift or glitches.
- [ ] No allocation or locking in the audio thread (code review plus an allocation-detecting allocator in tests).
- [ ] Offline-rendered crossfades match reference renders within tolerance.
- [ ] Streams simultaneously to at least two Icecast servers with different bitrates; disconnecting one does not affect the other.

## References

- [Audio engine](../audio-engine.md)
- [Encoders and outputs](../outputs.md)
- Open question: default encoder settings ([Open questions](../open-questions.md))
