# Roadmap

Work one milestone at a time, in order. Do not start a milestone before the previous one meets its acceptance criteria, unless the dependency column allows it. See [Workflow](../workflow.md).

Progress is tracked here: the status column below and the acceptance checklists in each milestone file.

| # | Milestone | Depends on | Status |
|---|---|---|---|
| M0 | [Foundations](m0-foundations.md) | — | Done ([#1](https://github.com/dehy/heron-broadcaster/pull/1)) |
| M1 | [Import & analysis](m1-import-analysis.md) | M0 | Not started |
| M2 | [Engine core](m2-engine-core.md) | M1 | Not started |
| M3 | [24/7 reliability](m3-reliability.md) | M2 | Not started |
| M4 | [API & control plane](m4-api-control-plane.md) | M2 | Not started |
| M5 | [Web UI](m5-web-ui.md) | M4 | Not started |
| M6 | [Rotation & clockwheels](m6-rotation-clockwheels.md) | M4 | Not started |
| M7 | [Webhooks & output API](m7-webhooks-output-api.md) | M4 | Not started |
| M8 | [Deck EQ](m8-deck-eq.md) | M2 | Not started |
| M9 | [AGC & multiband processing](m9-agc-multiband.md) | M8 | Not started |
| M10 | [More encoders, sinks & live DJ](m10-sinks-live-dj.md) | M3 | Not started |

## Dependency graph

```
M0 ─► M1 ─► M2 ─┬─► M3 ─► M10
                ├─► M4 ─┬─► M5
                │       ├─► M6
                │       └─► M7
                └─► M8 ─► M9
```
