# M3 – 24/7 reliability

**Depends on:** M2

## Scope

Already delivered in [M2](m2-engine-core.md): the queue lives in the engine's memory and tracks are decoded 10 s ahead, the engine never touches the database, outputs reconnect with exponential backoff, a cued file that cannot be decoded is skipped, and outputs and settings survive a restart. Left for this milestone:

- Fallback directory when the queue runs dry or the API is unreachable; silence watchdog on the output.
- Keeping more of the upcoming tracks ready (decoded or read into memory), so a slow or unreachable library does not cause underruns.
- Persist the queue and resume playback on restart.
- Docker restart policies and health checks.
- Clock sync monitoring in `heron-engine`: `heron_clock_synchronized` and `heron_clock_offset_seconds` in `/metrics`, and a warning in `/healthz` when the clock is not synced or is off by more than about 1 s (see [Requirements – Reliability](../requirements.md#5-reliability-247)). The reading itself (`heron_service::clock`) was added in [M5](m5-web-ui.md), whose dashboard shows the warning through `GET /api/v1/status`.

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
