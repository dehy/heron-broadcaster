# M9 – Deck EQ

**Depends on:** M2

## Why

Stations shape the sound of what they play: more bass for a dance format, less harshness on old masters, a voice EQ on liners and voice tracks. Until now Heron plays tracks as they are, apart from the loudness gain.

## Scope

- **Two 10-band EQs:** one for the music (Decks A and B), one for the FX player. Decks A and B take turns, so they share one EQ: a track sounds the same whichever deck it plays on. Bands at 60, 170, 310, 600, 1k, 3k, 6k, 12k, 14k and 16k Hz, ±12 dB each ([Requirements – Deck EQ](../requirements.md#deck-eq)): shelves at 60 Hz and 16 kHz, peaking bands as wide as their neighbours' distance in between, and filter gains solved so the curve meets every slider at its band.
- **Enable / bypass** for each EQ. A bypassed EQ is not computed: the audio is the same as without one.
- **Presets:** save the current sliders under a name, load a preset into either EQ, rename, delete. A fresh install has `Flat`, `Rock`, `Pop`, `Soft rock`, `Dance` and `Voice (FX)`, gentler than the classic graphic EQ presets since nothing limits boosts before M10.
- **Smoothed changes:** moving a slider, loading a preset, enabling or bypassing an EQ glide over about 50 ms, with no clicks.
- **In the engine:** an EQ on each deck slot, after the deck's gain (loudness and fades), before the deck meters and the master bus. The settings are kept in the engine's `config.json`, so it restarts and hands over with them. Design: [Audio engine – Deck EQ](../audio-engine.md#deck-eq).
- **API:** `GET`, `PUT /settings/eq`, presets at `/eq-presets` (operators). Pushed to the engine like the playout settings ([API – Engine configuration](../api.md#engine-configuration)).
- **Web UI:** a **Deck EQ** page with a panel of ten vertical sliders for each EQ, each with a number input for precise gains, enable / bypass, presets, and the response curve drawn as the sliders move. Other open pages follow the changes. An `EQ` badge on the decks of the On air page while their EQ is on and not flat.

## Out of scope

- Per-track EQ, and separate EQs for Deck A and Deck B.
- Parametric bands, other band frequencies or widths chosen by the user.
- Automatic headroom: boosts can take a loud track over full scale, where the master bus clips as it does today. The limiter comes with [M10](m10-agc-multiband.md).
- The bass EQ of the master bus, part of the AGC chain ([M10](m10-agc-multiband.md)).

## Deliverables

- `heron-dsp`: shelving and peaking filters, the 10-band graphic EQ with smoothing, and the filter gains solved from the sliders (`crates/dsp/src/eq.rs`). A fixture shared with the web UI's tests keeps the page's curve equal to the engine's.
- `heron-core`: `EqSettings`, the `eq` settings group.
- `heron-engine`: the EQ on each deck slot, the `SetEq` mixer command, the `SetEqSettings` gRPC call, `eq` in `config.json` and `GetConfig`, `heron-engine ctl set-eq`.
- `heron-api`: the `eq` settings group pushed to the engine, the `eq_presets` table (migration, six presets seeded), the routes, the `eq` live message.
- `heron-web`: the Deck EQ page, the `EQ` badge on the On air page, translations.
- Docs: [Audio engine – Deck EQ](../audio-engine.md#deck-eq), [API](../api.md#deck-eq), [Data model](../data-model.md#settings), [Web UI](../web.md#pages).

## Acceptance criteria

- [ ] Frequency response tests match expected curves: each band alone at ±12 dB and in between, and random settings of all ten, match the response of the filter design computed in `f64` within 0.1 dB from 20 Hz to 20 kHz. The curve meets each slider at its band's centre within 0.2 dB, or within 4 dB when the ±24 dB limit of the filter gains is reached; all sliders at the same gain give a curve within half that gain of it from 60 Hz to 16 kHz. A flat EQ changes nothing, and a bypassed one is bit-identical to no EQ. The web UI computes the same curve as the engine (a shared fixture). (`heron_dsp::eq::tests`: 23 settings × 31 frequencies; `heron-engine` mixer tests: a crossfade through the music EQ equals the crossfade without it, through one EQ.)
- [ ] No clicks when moving sliders during playback: sines rendered while a band goes from −12 to +12 dB and back, while a preset is loaded, and while the EQ is enabled and bypassed show no step (no sample-to-sample change larger than the steady sine's own), and their energy more than 500 Hz from the sine stays within 10 dB of an ideal glide's (the level and phase of the design moving with the gains, which no filter can follow instantly) and 30 dB under that of the same change made at once, which clicks. (`heron_dsp::eq::tests`: smoothed changes stay within 8 dB of the ideal glide and 35 to 55 dB under a jump, which a control test shows failing both checks. The first filter form, a direct form biquad, failed them: hence the state-variable filters.) Checked by ear on a stack too.
- [x] The audio thread still never allocates: `tests/no_alloc.rs` moves and toggles both EQs during crossfades.
- [x] A slider moved in the web UI is heard within 200 ms, and other open pages follow it. (Two key presses on a slider, the second held back by the 100 ms between requests: the engine had the final value 111 to 131 ms later, another open page showed it 128 to 133 ms later.)
- [x] Presets: save, load into either EQ, rename, delete, through the API and the web UI. `Flat`, `Rock`, `Pop`, `Soft rock`, `Dance` and `Voice (FX)` are there on a fresh install. (`e2e::deck_eq_and_presets`; in a browser: saved from the music EQ, loaded into the FX EQ, deleted.)
- [x] The EQ survives an engine restart and a hot handover ([M8](m8-hot-engine-upgrade.md)): the new engine plays with it from its first sample, with no glide from flat. (`tests/handover.rs`, the hand-over and the cold start after a given-up handover; `config::tests`; settings applied before the first sample are checked by the mixer's crossfade test, filtered from frame 0.)
- [x] An engine from before M9, started on the same state volume (a rollback), plays on, without EQ. (The engine of `main` before M9, on a state with `eq` in `config.json`: it resumed the saved track, no error. Hot handovers keep protocol 1, so either way works; an older engine handing over to M9 hands no EQ, and the API pushes it to the new engine as soon as it sees it.)

## References

- [Requirements – Deck EQ](../requirements.md#deck-eq)
- [Audio engine – Deck EQ](../audio-engine.md#deck-eq)
- [Audio engine – Signal quality](../audio-engine.md#signal-quality)
