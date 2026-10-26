# M7b – Listener statistics

**Depends on:** M4. The statistics page comes with [M5](m5-web-ui.md).

How many people listen, each day, each hour, to each output and to each track. Heron reads the statistics the stream servers already keep: listeners connect to Icecast or Shoutcast, never to Heron. Design: [Listener statistics](../statistics.md).

## Scope

### Collecting

- `heron-api` polls the server of every enabled output once a minute. The engine is not involved: it must stay focused on the air.
  - Icecast: listeners per mount from `/status-json.xsl` (public), and the list of connected listeners from `/admin/listclients` (needs the server's admin credentials, an optional setting of the output).
  - Shoutcast v1 and v2: their statistics pages, once the Shoutcast sinks exist ([M11](m11-sinks-live-dj.md)).
  - HLS has no server to ask: counting its listeners from the web server's access logs is left for later.
- A server that does not answer leaves a gap in the statistics, never an error on the air.

### What is measured

- **Concurrent listeners**, every minute, per output.
- **Listener sessions**, when the listener list is available: start, end, duration and player (user agent). A listener reconnecting within a minute continues the same session; a session going on at midnight is split there.
- **Listeners per day**: distinct listeners of the day, counted from sessions. Sessions shorter than a minimum duration (default 1 min, a setting) are not counted, like stream directories and audience measurers do. Weeks and months show the average per day: days cannot be linked.
- **Per day and per hour**: peak, average, total listening hours (TLH), number of sessions, average session length.
- **Per track**: listeners when the track started and when it ended (from the play history), and the sessions that ended during it, to spot the tracks that make people leave (by departure rate).
- Days and hours in the station's time zone; the whole station adds the outputs up minute by minute.

### Privacy

- IP addresses are never stored. A listener is identified within a day by a hash of their IP and player, salted with a key that changes every day, so days cannot be linked.
- Sessions and minute samples are kept 90 days (a setting); daily and hourly totals are kept as long as the station wants.
- The stream server's admin credentials are stored with the output, apart from what the engine gets, and never sent back.

### Showing

- Statistics page in the web UI: today live, then per day, per week, per hour of the day, per output, and the tracks with the most departures.
- API routes under `/api/v1/listeners`; CSV export of days, hours and tracks, for reporting to rights societies or advertisers.
- `heron_listeners{output}` in `/metrics`, and the current listener count on the dashboard.

## Acceptance criteria

- [x] Against a test Icecast server with simulated listeners (connections of known length), the concurrent counts, sessions, daily distinct listeners and listening hours match what was simulated; a reconnection within a minute continues its session, sessions under the minimum are not counted.
- [x] Against a real Icecast server (the `icecast` Compose profile) with a few players connected, the Statistics page shows them, live and in today's totals.
- [x] An Icecast server without admin credentials still gives concurrent listeners, peak, average and listening hours.
- [x] No IP address is stored anywhere in the database, and listener keys of two days differ for the same listener.
- [x] A stream server going down leaves a gap in the statistics, keeps the sessions of listeners still connected when it is back, and does not affect the air.
- [x] Departures are attributed to the track on air when sessions ended; CSV exports open in a spreadsheet with the same figures as the page.
- [x] Changing the statistics settings of an output does not reconnect it.

## References

- [Listener statistics](../statistics.md)
- [Encoders and outputs](../outputs.md)
- [Requirements – Outputs and integrations](../requirements.md#4-outputs-and-integrations)
- [Data model – Listener statistics](../data-model.md#listener-statistics)
- [Open questions](../open-questions.md)
