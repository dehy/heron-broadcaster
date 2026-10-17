# Open questions

Raise these when they become relevant instead of guessing.

| Question | Relevant to |
|---|---|
| HLS details: segment container (MPEG-TS/ADTS or fMP4), now-playing metadata in segments (ID3 timed metadata), and whether `heron-web` serves the HLS directory. | [M10](roadmap/m10-sinks-live-dj.md) |
| Name of the analysis service: `heron-analyzer` was chosen as a working name. | [M0](roadmap/m0-foundations.md) |
| Silence ceiling: −50 dBFS trims inaudible fade-ins but also quiet deliberate intros (Pink Floyd's wind, Yes' nature sounds, 23–27 s). −60 dBFS keeps most of them. Which default? | [M1](roadmap/m1-import-analysis.md) |
| AcoustID terms: its free application keys are for non-commercial use. Confirm stations bring their own key, and how commercial stations are handled. | [M1b](roadmap/m1b-library-management.md) |
| Default folder layout and whether cover art is embedded in files or only stored next to them. | [M1b](roadmap/m1b-library-management.md) |
| Opus and HE-AAC decoding, not supported by Symphonia: add libopus (BSD) and an HE-AAC-capable decoder, or report them as unsupported. | [M2](roadmap/m2-engine-core.md) |
| Restrictions: which kinds are needed first (dayparting, date ranges, category sequences, others such as "no two tracks over 8 min in an hour")? | [M6](roadmap/m6-rotation-clockwheels.md) |
| Artist names: should "featuring" tags be split automatically into several artists, and how are ambiguous names handled (`Simon & Garfunkel` is one artist)? | [M6](roadmap/m6-rotation-clockwheels.md) |
| Listener statistics: how long to keep listener sessions (daily totals are kept indefinitely), the minimum session length that counts as a listener (1 min proposed), and whether reports must follow a given audience-measurement standard. | [M7b](roadmap/m7b-listener-statistics.md) |

## Resolved

| Question | Answer | Recorded in |
|---|---|---|
| Authentication model: local users only, or OIDC as well? | **Local users only for now.** OIDC may come later. | [Decisions](decisions.md#decisions-made-after-the-handoff) |
| Does the analysis worker stay inside `heron-api` or become its own service? | **Its own modular service** (`heron-analyzer`). | [Decisions](decisions.md#decisions-made-after-the-handoff), [Architecture](architecture.md) |
| Rotation relaxation order. | Album → artist → title → track, each halved before being dropped, then the fallback category. | [Rotation](rotation.md#rule-relaxation) |
| Library organization and tag updates. | **Managed library** (incoming folder, Artist / Album / Track), identification **applied automatically when confident**, in a new milestone **M1b**. | [M1b](roadmap/m1b-library-management.md) |
| Default encoder profiles on a fresh install. | **MP3 128 kbps** (default for new outputs) and **AAC-LC 96 kbps** (default for HLS). | [Encoders and outputs](outputs.md#default-profiles) |
