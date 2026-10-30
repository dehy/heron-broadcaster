# Open questions

Raise these when they become relevant instead of guessing.

| Question | Relevant to |
|---|---|
| HLS details: segment container (MPEG-TS/ADTS or fMP4), now-playing metadata in segments (ID3 timed metadata), and whether `heron-web` serves the HLS directory. | [M11](roadmap/m11-sinks-live-dj.md) |
| Name of the analysis service: `heron-analyzer` was chosen as a working name. | [M0](roadmap/m0-foundations.md) |
| Silence ceiling: −50 dBFS trims inaudible fade-ins but also quiet deliberate intros (Pink Floyd's wind, Yes' nature sounds, 23–27 s). −60 dBFS keeps most of them. Which default? | [M1](roadmap/m1-import-analysis.md) |
| Opus and HE-AAC decoding, not supported by Symphonia: add libopus (BSD) and an HE-AAC-capable decoder, or report them as unsupported. | [M2](roadmap/m2-engine-core.md) |

## Resolved

| Question | Answer | Recorded in |
|---|---|---|
| Liners of a medley tagged with several stations. | **Its first station's**: one rendered file, with the liners of the station it was made on, played as it is on its other stations. | [Several stations](stations.md#medleys) |
| Listener statistics: how long to keep sessions, the minimum session length that counts as a listener, and whether reports follow an audience-measurement standard. | **Sessions (and minute samples) kept 90 days**, a setting; daily and hourly totals kept. **1 min minimum**, a setting. **No standard**: the measures are defined in the docs, with a CSV export. | [Listener statistics](statistics.md) |
| Authentication model: local users only, or OIDC as well? | **Local users only for now.** OIDC may come later. | [Decisions](decisions.md#decisions-made-after-the-handoff) |
| Does the analysis worker stay inside `heron-api` or become its own service? | **Its own modular service** (`heron-analyzer`). | [Decisions](decisions.md#decisions-made-after-the-handoff), [Architecture](architecture.md) |
| Rotation relaxation order. | Album → artist → title → track, each halved before being dropped, then the fallback category. | [Rotation](rotation.md#rule-relaxation) |
| Library organization and tag updates. | **Managed library** (incoming folder, Artist / Album / Track), identification **applied automatically when confident**, in a new milestone **M1b**. | [M1b](roadmap/m1b-library-management.md) |
| AcoustID terms: free application keys are for non-commercial use. | **Each station sets its own key** (commercial stations get a commercial one). Without a key, identification is off: files are filed using their own tags and nothing is flagged for review. | [Library management](library.md#identification) |
| Default folder layout, and cover art embedded or next to the files. | **`{album_artist}/{album}/{disc-}{track:02} {title}.{ext}`** (configurable). Cover art is saved as **`cover.jpg` in the album folder**; embedding in the files is optional and off by default. | [Library management](library.md#layout) |
| Which service moves files and writes tags. | A **new service, `heron-librarian`**, with the only read-write mount of the library; `heron-api` stays read-only. | [Architecture](architecture.md) |
| Default encoder profiles on a fresh install. | **MP3 128 kbps** (default for new outputs) and **AAC-LC 96 kbps** (default for HLS). | [Encoders and outputs](outputs.md#default-profiles) |
| Restrictions: which kinds are needed first? | **Dayparting, date ranges and category sequences**; quotas later if needed. | [Rotation](rotation.md#restrictions) |
| Artist names: split "featuring" automatically, and how are ambiguous names handled? | **Split on featuring only** (`feat.`, `ft.`, `featuring`, also in titles); `&`, `and` and commas never split (`Simon & Garfunkel` is one artist). **Similar artists** count as one for separation; links are suggested (names, MusicBrainz) and reviewed by hand. | [Rotation](rotation.md#artists-with-several-names) |
| What counts as a play for play counts and last played. | **A minimum played time, 30 s by default**, a station setting (0 counts every start); the FX player never counts. | [API](api.md#track-details) |
| Time zone of the schedule. | A **station setting** (IANA name), the server's by default. | [Rotation](rotation.md#scheduling) |
| Track management: upload limit, buffer quota, trash retention, who deletes, whether buffer files expire. | **2 GiB per file, and keep 5 GiB free** on the buffer zone's disk (no separate quota); the trash keeps tracks **30 days**; **operators delete and restore, admins empty the trash**; files in the buffer **never expire**. All but the roles are settings. | [Library management](library.md#the-buffer-zone) |
