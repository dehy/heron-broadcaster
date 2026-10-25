# M5 – Web UI

**Depends on:** M4

## Scope

- Live dashboard: Deck A / Deck B, progress, levels, now playing.
- Dashboard warning when the server clock is not synced or is off by more than about 1 s (from the clock sync monitoring added in M3).
- Queue editing with drag and drop; library browser with search and filters.
- Category management; per-track settings (cue points, crossfade overrides).
- Review screen for identifications that were not confident enough to apply automatically, and for library reorganization dry runs ([M1b](m1b-library-management.md)).
- Waveform view showing cue-in, cue-out and fade points.
- Crossfade settings with a live preview graph.
- Encoder profiles and outputs editor, with each output's state (connected, reconnecting, error).
- Light and dark themes that **follow the system setting** (`prefers-color-scheme`) by default, with a System / Light / Dark override remembered per user.
- Responsive layout.

## Acceptance criteria

- [x] A station can be operated entirely from the browser.

  Demonstrated against a running station (API, engine, analyzer and librarian, with a library of generated tracks), driving Chrome headless: signing in; skip, pause and resume; reordering the queue by dragging, queuing from a search, play next; dragging a track's cue-in on its waveform and saving it, then going back to the defaults; adding an Icecast output and seeing its state and error; an organize dry run, its apply and its undo; keeping a track's own tags on the review screen; creating a category; switching to the dark theme (saved for the user); the phone layout without horizontal scrolling; a viewer seeing no controls. Only creating the first admin stays on the command line (`heron-api user add`), on purpose.

## Results

- Design and pages: [Web UI](../web.md).
- **API additions** ([API](../api.md)): each user's theme (`PUT /auth/preferences`); the server clock's synchronization in `GET /status` (read-only `adjtimex` in `heron_service::clock`, which [M3](m3-reliability.md) can reuse for the engine's metrics); `GET /tracks/{id}/waveform` and `/tracks/{id}/audio` (range requests); library management.
- **Library management through the API:** the API cannot write files, so the review of identifications (apply a candidate release, or keep the file's tags), organizing after a dry run and undoing batches are **library tasks** (`library_tasks`) that `heron-librarian` runs in order ([Library management](../library.md#work-asked-through-the-api)). A picked release is applied as a `review` batch; undoing it puts the track back in review.
- **Waveforms** are computed with the analysis (`track_waveforms`, one byte per 50 ms on a dB scale). `ALGORITHM_VERSION` 3: upgrading re-analyzes the library once, in the background.
- **Web app:** React with TanStack Query and dnd-kit, a small History API router, live state from the WebSocket, light and dark themes following the system with a per-user override, responsive down to phone width. Unit tests with Vitest in CI.
- Deferred: tag editing in the browser ([M5b](m5b-track-details-tags.md)); forms for outputs and encoders other than Icecast and MP3 ([M11](m11-sinks-live-dj.md)); clockwheel and rotation screens (delivered in [M6](m6-rotation-clockwheels.md)); a browser end-to-end test in CI (the demonstration above was run by hand).
- Not verified here: the Compose images were not rebuilt locally (Docker had no outbound network during the work); CI builds and runs them.

## References

- [Requirements – Playout](../requirements.md#2-playout)
- [Conventions – Web](../conventions.md#web)
