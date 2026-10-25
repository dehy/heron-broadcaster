# M10 – AGC & multiband processing

**Depends on:** M9

## Scope

- Master chain in the order listed in [Audio processing](../requirements.md#master-agc-chain).
- Compressor / expander / limiter modes with ratio, threshold, attack, release, hold, band gain, band links.
- Linkwitz-Riley crossovers.
- Presets, bypass all, in/out and per-band meters.

## Acceptance criteria

- [ ] Each block is unit-tested with test signals.
- [ ] No audible artifacts on a reference playlist.
- [ ] CPU usage measured and documented.

## References

- [Requirements – Master AGC chain](../requirements.md#master-agc-chain)
- [Audio engine](../audio-engine.md)
