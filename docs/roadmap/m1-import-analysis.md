# M1 – Import & analysis

**Depends on:** M0

## Scope

- Library scanner: initial scan plus file watcher, tag reading.
- Per-track analysis stored in MariaDB:
  - duration;
  - integrated loudness and true peak;
  - cue-in / cue-out (Gap Killer thresholds, separate for start and end);
  - blip detection;
  - fade-out start point (dB trigger, min/max fade time);
  - long internal silence markers.
- Analysis versioning, so tracks can be re-analyzed when settings change.
- Worker pool in its own service, `heron-analyzer`; analysis never runs in the engine process. The scanner (in `heron-api`) enqueues jobs, analyzer replicas consume them (see [Architecture](../architecture.md#scanner-and-analyzer-split)).

## Acceptance criteria

- [x] Importing 1 000 tracks completes and every track has analysis data.
- [x] Re-scanning does not re-analyze unchanged files.
- [x] Unit tests with generated fixtures (silence at start/end, blips, quiet outro).

## Results

- 1,000 generated tracks (MP3, M4A, FLAC; `scripts/generate-test-library.sh`): imported in 2 s, all analyzed in about 35 s on a laptop. Cue points within 5 ms of the generated silence for MP3 and FLAC, about 25 ms for M4A (AAC priming). A rescan takes 83 ms and queues nothing. CI repeats this with 60 tracks.
- 4,364 real tracks read over an SMB share while they were still being copied: 4,354 analyzed, 83 correctly found silent (an album of 4 s silent tracks), 10 failed on files that are actually broken (truncated, corrupt, MP3 inside WAV). See [Import and analysis](../analysis.md). This led to the absolute silence ceiling, ID3v2 support, the gapless fallback, the rejection of corrupt decodes, the 64 KiB fingerprint, and postponing jobs whose file is still changing.

## References

- [Requirements – Library](../requirements.md#1-library-and-categories), [Crossfading](../requirements.md#crossfading), [Gap Killer](../requirements.md#gap-killer)
- [Data model – Library](../data-model.md#library)
- [Architecture – Scanner and analyzer split](../architecture.md#scanner-and-analyzer-split)
- [Import and analysis](../analysis.md)
