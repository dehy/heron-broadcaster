# Open questions

Raise these when they become relevant instead of guessing.

| Question | Relevant to |
|---|---|
| Exact relaxation order for rotation rules (initial proposal in [Rotation](rotation.md#rule-relaxation)). | [M6](roadmap/m6-rotation-clockwheels.md) |
| Default encoder profiles created on a fresh install: which codecs (MP3, AAC-LC, Opus) and bitrates. | [M2](roadmap/m2-engine-core.md) |
| HLS details: segment container (MPEG-TS/ADTS or fMP4), now-playing metadata in segments (ID3 timed metadata), and whether `heron-web` serves the HLS directory. | [M10](roadmap/m10-sinks-live-dj.md) |
| Name of the analysis service: `heron-analyzer` was chosen as a working name. | [M0](roadmap/m0-foundations.md) |

## Resolved

| Question | Answer | Recorded in |
|---|---|---|
| Authentication model: local users only, or OIDC as well? | **Local users only for now.** OIDC may come later. | [Decisions](decisions.md#decisions-made-after-the-handoff) |
| Does the analysis worker stay inside `heron-api` or become its own service? | **Its own modular service** (`heron-analyzer`). | [Decisions](decisions.md#decisions-made-after-the-handoff), [Architecture](architecture.md) |
