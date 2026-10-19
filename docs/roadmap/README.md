# Roadmap

Work one milestone at a time, in order. Do not start a milestone before the previous one meets its acceptance criteria, unless the dependency column allows it. See [Workflow](../workflow.md).

Progress is tracked here: the status column below and the acceptance checklists in each milestone file.

| # | Milestone | Depends on | Status |
|---|---|---|---|
| M0 | [Foundations](m0-foundations.md) | — | Done ([#1](https://github.com/dehy/heron-broadcaster/pull/1)) |
| M1 | [Import & analysis](m1-import-analysis.md) | M0 | Done ([#3](https://github.com/dehy/heron-broadcaster/pull/3)) |
| M1b | [Library management](m1b-library-management.md) | M1 | Done |
| M2 | [Engine core](m2-engine-core.md) | M1 | In progress: 24 h run pending |
| M3 | [24/7 reliability](m3-reliability.md) | M2 | Done |
| M4 | [API & control plane](m4-api-control-plane.md) | M2 | Done |
| M5 | [Web UI](m5-web-ui.md) | M4 | Done |
| M5b | [Track details, labels & tags](m5b-track-details-tags.md) | M5 | Done |
| M5c | [Track management](m5c-track-management.md) | M5 | Done ([#18](https://github.com/dehy/heron-broadcaster/pull/18)) |
| M5d | [Web UI translations](m5d-web-i18n.md) | M5 | Done ([#17](https://github.com/dehy/heron-broadcaster/pull/17)) |
| M6 | [Rotation & clockwheels](m6-rotation-clockwheels.md) | M4 | Done ([#11](https://github.com/dehy/heron-broadcaster/pull/11)) |
| M7 | [Webhooks & output API](m7-webhooks-output-api.md) | M4 | Not started |
| M7b | [Listener statistics](m7b-listener-statistics.md) | M4 | Not started |
| M8 | [Deck EQ](m8-deck-eq.md) | M2 | Not started |
| M9 | [AGC & multiband processing](m9-agc-multiband.md) | M8 | Not started |
| M10 | [More encoders, sinks & live DJ](m10-sinks-live-dj.md) | M3 | Not started |

## Dependency graph

```
M0 ─► M1 ─┬─► M1b
          └─► M2 ─┬─► M3 ─► M10
                  ├─► M4 ─┬─► M5 ─┬─► M5b
                  │       │       ├─► M5c
                  │       │       └─► M5d
                  │       ├─► M6
                  │       ├─► M7
                  │       └─► M7b
                  └─► M8 ─► M9
```
