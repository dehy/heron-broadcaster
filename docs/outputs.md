# Encoders and outputs

How audio leaves `heron-engine`. Outputs are pluggable ("bring your own streamer"), and a station can feed **several stream servers at once**, each with its own encoder settings.

## Model

```
                                  ┌──────────────────────┐     ┌─────────────────────────┐
                              ┌──►│ Encoder: MP3 128k    │──┬─►│ Output: Icecast A /live │
                              │   └──────────────────────┘  └─►│ Output: Shoutcast B     │
 master bus ──► encoder fan-out   ┌──────────────────────┐     └─────────────────────────┘
 (f32 stereo 48 kHz)          ├──►│ Encoder: AAC-LC 96k  │──┬─►│ Output: Icecast C /aac  │
                              │   └──────────────────────┘  └─►│ Output: HLS             │
                              │   ┌──────────────────────┐     └─────────────────────────┘
                              └──►│ Encoder: external    │────►│ Output: Icecast D /he   │
                                  │ ffmpeg (user binary) │     └─────────────────────────┘
                                  └──────────────────────┘
```

- An **encoder profile** describes one encoding: encoder kind, bitrate, sample rate, channels and encoder-specific options.
- An **output** is one destination: a sink kind, its connection settings (host, port, mount, credentials…), the encoder profile it uses, metadata settings and an enabled flag.
- Several outputs can share one encoder profile: identical encodings are produced **once** and fanned out to every output that uses them.
- Any number of outputs can be configured, to the same or different stream servers.

## Encoders

The user picks the encoder for each profile.

| Encoder | Codec | Implementation | License |
|---|---|---|---|
| **MP3** | MP3 (CBR/VBR) | Built in, LAME | LGPL |
| **AAC** | AAC-LC | Built in, FFmpeg's native `aac` encoder (libavcodec built without `--enable-nonfree`) | LGPL (GPL builds are also compatible) |
| **Opus** | Opus (Ogg) | Built in, libopus | BSD |
| **Vorbis** | Vorbis (Ogg) | Built in, libvorbis | BSD |
| **External FFmpeg** | Anything the provided binary supports, e.g. HE-AAC v1/v2 with `libfdk_aac` | User-provided `ffmpeg` binary, run as a child process | User's responsibility |

### Built-in AAC

Plain AAC-LC without HE (SBR/PS) is sufficient for current players and bitrates, so HE-AAC is not a built-in feature.

### External FFmpeg encoder

- Heron **never ships, builds or links fdk-aac**. Selecting an fdk-aac codec requires the user to provide their own fdk-aac-enabled `ffmpeg` binary (configured path, e.g. mounted into the `heron-engine` container or in a custom image).
- The engine spawns the binary, writes raw PCM (f32le, stereo, 48 kHz) to its stdin and reads the encoded stream from its stdout, which is then sent to the outputs like any built-in encoder.
- Heron checks at configuration time that the binary exists and supports the chosen codec (`ffmpeg -encoders`), and reports a clear error otherwise.
- If the process dies, it is restarted with backoff; other encoders and outputs are not affected.

## Outputs (sinks)

All sinks implement the `OutputSink` trait: they receive encoded frames and now-playing metadata.

| Sink | Notes |
|---|---|
| **Icecast** | Source client, metadata updates. First sink to be built. |
| **Shoutcast v1/v2** | Source client, metadata updates. |
| **HLS** | Segments the encoded stream (AAC-LC by default; MP3 also possible) and writes segments plus a rolling `.m3u8` playlist to a directory, to be served by any HTTP server or CDN. Configurable segment duration and playlist window. |
| **File recording** | Writes the encoded stream to files, with rotation. |
| **Pipe** | Sends an encoded stream to an external process. |

## Isolation and reliability

- Encoders and outputs run **outside the audio thread**. The mixer writes to a lock-free ring buffer per encoder; a slow or failed encoder or output never blocks the mixer or other outputs.
- Each network output reconnects on its own with exponential backoff.
- An output's state (connected, reconnecting, error, bytes sent, listeners when the server reports them) is exposed through gRPC, the API and `/metrics`.

## Configuration flow

Encoder profiles and outputs are edited in the web UI and stored by `heron-api` in MariaDB. The API pushes the configuration to the engine over gRPC; the engine keeps a local copy so it restarts with its outputs even when the API or database is down (the engine never reads the database, see [Locked decisions](decisions.md)).

## Delivery

- [M2 – Engine core](roadmap/m2-engine-core.md): encoder/output model, multiple outputs, MP3 encoder, Icecast sink.
- [M10 – More encoders, sinks & live DJ](roadmap/m10-sinks-live-dj.md): AAC-LC, Opus, Vorbis, external FFmpeg encoder, HLS, Shoutcast, file recording, pipe.
