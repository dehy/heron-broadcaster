# M3 – 24/7 reliability

**Depends on:** M2

## Scope

- In-memory buffer of upcoming tracks; no database access in the audio path.
- Fallback directory, silence watchdog, skip on missing or corrupt files.
- Icecast reconnect with exponential backoff.
- Persisted engine state and resume on restart.
- Docker restart policies and health checks.

## Acceptance criteria

- [ ] Chaos tests — in every case the stream continues or recovers automatically:
  - [ ] stop MariaDB;
  - [ ] stop the API;
  - [ ] restart Icecast;
  - [ ] delete a queued file;
  - [ ] inject a silent file.
- [ ] An engine restart resumes playback within a few seconds.

## References

- [Requirements – Reliability](../requirements.md#5-reliability-247)
- [Audio engine – Failure handling](../audio-engine.md#failure-handling)
