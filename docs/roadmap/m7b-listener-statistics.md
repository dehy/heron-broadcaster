# M7b – Listener statistics

**Depends on:** M4. The statistics page comes with [M5](m5-web-ui.md).

How many people listen, each day, each hour, to each output and to each track. Heron reads the statistics the stream servers already keep: listeners connect to Icecast or Shoutcast, never to Heron.

## Scope

### Collecting

- `heron-api` polls the server of every enabled output once a minute. The engine is not involved: it must stay focused on the air.
  - Icecast: listeners per mount from `/status-json.xsl` (public), and the list of connected listeners from `/admin/listclients` (needs the server's admin credentials, an optional setting of the output).
  - Shoutcast v1 and v2: their statistics pages, once the Shoutcast sinks exist ([M10](m10-sinks-live-dj.md)).
  - HLS has no server to ask: counting its listeners from the web server's access logs is left for later.
- A server that does not answer leaves a gap in the statistics, never an error on the air.

### What is measured

- **Concurrent listeners**, every minute, per output.
- **Listener sessions**, when the listener list is available: start, end, duration and player (user agent). A listener reconnecting within a minute continues the same session.
- **Listeners per day**: distinct listeners of the day, counted from sessions. Sessions shorter than a minimum duration (default 1 min) are not counted, like stream directories and audience measurers do.
- **Per day and per hour**: peak, average, total listening hours (TLH), number of sessions, average session length.
- **Per track**: listeners when the track started and when it ended (from the play history), to spot the tracks that make people leave.

### Privacy

- IP addresses are never stored. A listener is identified within a day by a hash of their IP and player, salted with a key that changes every day, so days cannot be linked.
- Sessions are kept for a limited time; daily and hourly totals are kept as long as the station wants.

### Showing

- Statistics page in the web UI: today live, then per day, per week, per hour of the day, per output, and the tracks with the most departures.
- CSV export, for reporting to rights societies or advertisers.
- `heron_listeners{output}` in `/metrics`, and the current listener count on the dashboard.

## Acceptance criteria

- [ ] Against a test Icecast server with simulated listeners (connections of known length), the concurrent counts, sessions, daily distinct listeners and listening hours match what was simulated.
- [ ] An Icecast server without admin credentials still gives concurrent listeners, peak, average and listening hours.
- [ ] No IP address is stored anywhere in the database.
- [ ] A stream server going down leaves a gap in the statistics and does not affect the air.

## References

- [Encoders and outputs](../outputs.md)
- [Requirements – Outputs and integrations](../requirements.md#4-outputs-and-integrations)
- [Data model – Listener statistics](../data-model.md#listener-statistics)
- [Open questions](../open-questions.md)
