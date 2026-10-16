# M3 – 24/7 reliability

**Depends on:** M2

## Scope

- In-memory buffer of upcoming tracks; no database access in the audio path.
- Fallback directory, silence watchdog, skip on missing or corrupt files.
- Icecast reconnect with exponential backoff.
- Persisted engine state and resume on restart.
- Docker restart policies and health checks.
- Clock sync monitoring in `heron-engine`: `heron_clock_synchronized` and `heron_clock_offset_seconds` in `/metrics`, and a warning in `/healthz` when the clock is not synced or is off by more than about 1 s (see [Requirements – Reliability](../requirements.md#5-reliability-247)).

## Acceptance criteria

- [ ] Chaos tests — in every case the stream continues or recovers automatically:
  - [ ] stop MariaDB;
  - [ ] stop the API;
  - [ ] restart Icecast;
  - [ ] delete a queued file;
  - [ ] inject a silent file.
- [ ] An engine restart resumes playback within a few seconds.
- [ ] With time sync stopped on the host, `heron_clock_synchronized` drops to 0 and `/healthz` reports a clock warning.

## References

- [Requirements – Reliability](../requirements.md#5-reliability-247)
- [Audio engine – Failure handling](../audio-engine.md#failure-handling)
