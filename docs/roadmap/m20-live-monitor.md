# M20 – Live monitor

**Depends on:** M10 (the AGC chain), M13 (several stations)

## Why

Tuning the sound of a station by ear is done today through its stream: an Icecast mount played by the browser. Between a control moved and its effect heard, 8 to 10 seconds pass (the encoder, Icecast's burst, the browser's buffer). Comparing with and without processing means bypassing the chain on air. Both make tuning slow and risky.

## Scope

- **A live monitor**: the station's sound, straight from its engine, in the browser, a fraction of a second behind the air.
- **Processed or raw**: what the encoders get (after the AGC chain and the clip), or the master bus before the chain (decks, fades, loudness gain, deck EQ, FX player). Switching between them is instant and seamless, and never touches the air.
- **Two qualities**: Opus at 256 kbit/s (about 0.3 s, at a distance too) by default, or uncompressed 16-bit PCM (about 0.15 s, 1.5 Mbit/s: on a local network).
- **One button at the top of the menu** (in the top bar on a phone) for operators and admins: it keeps playing from page to page (Processing, Deck EQ, Crossfade & loudness…), with its volume.
- **The engine taps the master bus** only while someone listens: the audio thread writes each block before and after the chain into a ring, sample-aligned, and a monitor thread streams it to the API over gRPC (`WatchMonitor`). Nothing changes on air, listened to or not.
- **The API** serves it as a WebSocket, `/stations/{id}/monitor`, encoding Opus with libopus.

## Out of scope

- Listening to an output as encoded (MP3, AAC): the outputs' own *Listen* links do that, with their delay.
- Recording, or listening to a deck or the FX player alone.
- Viewers: the monitor is for those who tune the sound.

## Deliverables

- Engine: the tap in the mixer, the monitor thread, `WatchMonitor` in `engine.proto`.
- API: the monitor WebSocket, Opus or PCM, processed or raw, at most 8 listeners per station.
- Web: the *Live monitor* button and its panel; Opus decoded with WebCodecs, PCM played as it comes.
- Docs: [Audio engine – Live monitor](../audio-engine.md#live-monitor), [API – Live monitor](../api.md#live-monitor), [Web UI](../web.md#live-monitor).
- libopus built with the API (the `opus` crate, statically linked): CMake in the build image.

## Acceptance criteria

- [ ] While nobody listens, the audio thread does nothing more than read a flag; the audio is bit-identical, listened to or not (`the_monitor_never_changes_the_air`).
- [ ] The raw and processed taps are sample-aligned: with the chain bypassed they are equal; with it on, the processed tap is what the encoders get.
- [ ] The audio thread still never allocates with the tap on (`tests/no_alloc.rs`).
- [ ] A listener that falls behind loses audio, never stalls the engine or another listener.
- [ ] The WebSocket answers `403` to viewers, sends Opus packets or PCM chunks of 20 ms, switches between raw and processed on the next 20 ms, and refuses a ninth listener of a station.
- [ ] In the browser, a control moved on the Processing page is heard within half a second with Opus (a quarter with PCM), on a local network.
- [ ] Switching raw and processed has no gap or click; the monitor keeps playing across pages, and stops when the session ends or the engine goes away, saying so.
- [ ] All four catalogs.
