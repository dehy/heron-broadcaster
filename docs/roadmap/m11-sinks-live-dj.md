# M11 – More encoders, sinks & live DJ

**Depends on:** M3

## Scope

- Encoders (see [Encoders and outputs](../outputs.md#encoders)):
  - AAC-LC, built in (FFmpeg's native `aac` encoder, built without `--enable-nonfree`);
  - migration seeding the default `AAC 96k` profile;
  - Opus and Vorbis;
  - external FFmpeg encoder using a user-provided binary (for fdk-aac / HE-AAC), with a capability check and restart on failure. *Done ahead of the rest of M11, at a station's request (see [Encoders and outputs](../outputs.md#external-ffmpeg-encoder)).*
- Sinks:
  - HLS: segments + rolling playlist written to a directory;
  - Shoutcast v1/v2;
  - file recording;
  - external process pipe.
- Live DJ input (Icecast source protocol) with smooth takeover and return to automation.

## Acceptance criteria

- [ ] Each built-in encoder streams to Icecast and plays in common players (browser, VLC, mobile).
- [ ] The HLS output plays in Safari and hls.js, and the playlist keeps rolling for 24 h.
- [ ] With a user-provided fdk-aac `ffmpeg`, an HE-AAC output works; with a binary lacking `libfdk_aac`, a clear error is reported and other outputs keep streaming.
- [x] Killing the external `ffmpeg` process recovers automatically without affecting other outputs.
- [ ] Switching between live and automation happens without dead air.

## References

- [Encoders and outputs](../outputs.md)
- [Locked decisions](../decisions.md) (codec licensing)
