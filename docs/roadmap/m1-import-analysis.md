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

- [ ] Importing 1 000 tracks completes and every track has analysis data.
- [ ] Re-scanning does not re-analyze unchanged files.
- [ ] Unit tests with generated fixtures (silence at start/end, blips, quiet outro).

## References

- [Requirements – Library](../requirements.md#1-library-and-categories), [Crossfading](../requirements.md#crossfading), [Gap Killer](../requirements.md#gap-killer)
- [Data model – Library](../data-model.md#library)
- [Architecture – Scanner and analyzer split](../architecture.md#scanner-and-analyzer-split)
