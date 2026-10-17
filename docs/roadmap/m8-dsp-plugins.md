# M8 – DSP plugins (Stereo Tool)

**Depends on:** M2

Heron's audio processing comes from DSP plugins. The built-in deck EQ and AGC chain are deferred ([Requirements – Deferred](../requirements.md#deferred)).

Process the master bus with a DSP plugin before it is encoded. The first backend is **Stereo Tool**, through its generic plugin library (libStereoTool). The Winamp DSP plugin (`dsp_stereo_tool.dll`) is Windows-only and needs a Winamp host, so it is not used: libStereoTool has the same processing, the same `.sts` presets and the same license key, and is built for every platform Heron runs on.

| Platform | Library (Stereo Tool generic plugin, `Stereo_Tool_Generic_plugin.zip`) |
|---|---|
| Linux amd64 | `libStereoTool_intel64.so` |
| Linux arm64 (Raspberry Pi 3–5) | `libStereoTool_noX11_arm64.so` |
| macOS (development) | `libStereoTool_64.dylib` |

Heron **never ships libStereoTool** (proprietary, incompatible with AGPL): the station downloads it from [Thimeo](https://www.thimeo.com/stereo-tool/download/) and mounts it, with its own license key.

## Scope

### `heron-dsp-host`, its own service

- A new service and crate, `heron-dsp-host`, runs the processor. A crash or hang in a closed-source library must never take the air down, so it never runs inside `heron-engine`.
- Backends sit behind a `Processor` trait (process a block of interleaved f32 stereo at 48 kHz in place, report the latency in frames, name and version). Other formats can be added later without touching the engine.
  - `stereotool`: loads libStereoTool at runtime (`libloading`). The only `unsafe` code in the workspace, allowed in this crate alone.
  - `delay`: passes audio through with a set latency. Used by the tests and in CI, where libStereoTool is not available.
- Stereo Tool setup:
  - `HERON_STEREOTOOL_LIBRARY` (path to the `.so`), `HERON_STEREOTOOL_KEY` (license key, a secret), optional preset `.sts` file.
  - `stereoTool_Create3` without GUI, with its settings file in the service's state directory, so changes made in Stereo Tool's web interface survive a restart.
  - Check `stereoTool_GetApiVersion` at load and refuse versions without the functions used. The `ID_SAVE_*` preset load types are positions in `ParameterEnum.h`: take them from the header of the supported version.
  - Latency from `stereoTool_GetLatency2`, after each preset load.
  - Report `stereoTool_GetUnlicensedUsedFeatures` (an unlicensed feature adds audible marks to the stream) in `/healthz` and `/metrics`.
  - Stereo Tool's own web interface for adjusting the sound live: bound with `stereoTool_global_setBindIp`, port published by Compose only when the station enables it.
- `/healthz`, `/metrics` (`heron_dsp_process_seconds`, `heron_dsp_latency_frames`, `heron_dsp_licensed`).

### Engine

- A processing stage between the audio thread and the encoder fan-out. **Never on the audio thread:** it sends blocks to `heron-dsp-host` and takes back the processed blocks.
- Transport: a Unix socket on a volume shared by the two services. Handshake with the protocol version, format and latency, then framed blocks with a sequence number.
- **Bypass that never skips or repeats audio:** the engine delays the raw master bus by the processor's latency, so the raw and processed signals stay aligned. When a processed block is not back within its deadline (host down, hung, too slow), the engine sends the aligned raw block instead, with a 10 ms crossfade on every switch either way. It reconnects in the background.
- Manual bypass (gRPC, API, web UI) uses the same path.
- **Now-playing metadata is delayed by the latency**, so titles change on the outputs with the audio. Positions and levels shown in the UI stay those of the mixer.
- Settings under `dsp` (enabled, socket path, deadline), stored in the engine's `config.json` and pushed by the API like the outputs.
- Metrics: `heron_dsp_connected`, `heron_dsp_bypassed_blocks_total`, `heron_dsp_reconnects_total`.

### API and web UI

- Enable or bypass the processor, and show its state: connected, backend and version, latency, license warnings, link to Stereo Tool's web interface when enabled.

### Deployment and docs

- `heron-dsp-host` in Compose, disabled unless a library is configured, with a restart policy. Images built for amd64 and arm64, without libStereoTool.
- Documented setup: download, mount, license key, preset, web interface.

### Out of scope

- Per-output processing (one processor per encoder profile).
- FM/MPX output (`stereoTool_ProcessFM`) and RDS.
- Other plugin formats (LV2, LADSPA, VST3, CLAP): possible later backends.

## Acceptance criteria

- [ ] With the `delay` backend, the stream equals the master bus delayed by the reported latency, sample for sample.
- [ ] Killing `heron-dsp-host` mid-stream, or freezing it (`SIGSTOP`), keeps the stream going on raw audio within one deadline: no gap, no skipped or repeated samples (checked offline against the master bus). When the host comes back, processed audio returns, crossfaded.
- [ ] A missing library, a bad license key or an unsupported API version leaves the engine streaming raw audio, and the UI says why.
- [ ] Now-playing metadata on Icecast changes with the audio, within one block, at a known latency.
- [ ] `tests/no_alloc.rs` still passes: the audio thread does not allocate.
- [ ] With libStereoTool, by hand on amd64 and on a Raspberry Pi (arm64): the library loads, the preset applies, the web interface changes the sound live, and settings survive a restart.
- [ ] CPU usage of a typical preset measured on a Raspberry Pi and documented.

## References

- [Requirements – DSP plugins](../requirements.md#dsp-plugins)
- [Audio engine – Real-time safety](../audio-engine.md#real-time-safety)
- [Encoders and outputs](../outputs.md)
- [Stereo Tool downloads](https://www.thimeo.com/stereo-tool/download/), [Liquidsoap's Stereo Tool integration](https://www.liquidsoap.info/doc-dev/stereotool.html)
