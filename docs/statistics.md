# Listener statistics

How many people listen, each day, each hour, to each output and to each track. Built in [M7b](roadmap/m7b-listener-statistics.md).

Listeners connect to the stream servers (Icecast, later Shoutcast), never to Heron, so Heron reads the statistics those servers keep. `heron-api` collects them; the engine is not involved and never waits for a stream server's statistics.

## Collecting

Once a minute, `heron-api` asks the server of every enabled Icecast output:

| What | Where | Needs |
|---|---|---|
| Listeners per mount | `/status-json.xsl` | nothing: the page is public |
| Connected listeners: address, player (user agent), connected for how long, the server's ID for the connection | `/admin/listclients?mount=…` | the server's **admin** credentials, an optional setting of the output |

The server is reached at the output's `host` and `port`, like the engine reaches it. Outputs on the same server share one request for the listener counts.

Without admin credentials, Heron still has the **concurrent listeners** of each minute, and from them the peak, the average and the listening hours. With them, it also has each listener's **sessions**, and from them the distinct listeners of a day, the number and length of sessions, the players, and the listeners who left during each track.

Shoutcast v1 and v2 servers will be read the same way once their outputs exist ([M11](roadmap/m11-sinks-live-dj.md)). HLS has no server to ask: counting its listeners from the web server's access logs is left for later.

### Gaps

A server that does not answer within 10 seconds, or answers something unexpected, leaves that minute **without a sample**: a gap, shown as such, never a zero. Nothing about it reaches the air: the engine does not know. The statistics page and `GET /listeners/now` tell which outputs could not be read and why; `heron_api_listener_polls_total{result="error"}` counts the failures.

Listeners connected through a gap keep their session: the server's ID for their connection tells it is the same one. Listeners gone when the server answers again end at the last minute they were seen.

## What is measured

| Measure | Defined as | Needs admin credentials |
|---|---|---|
| **Concurrent listeners** | Listeners of an output at a minute, as the server counts them | no |
| **Peak** | Highest concurrent listeners of the period | no |
| **Average** | Mean of the concurrent listeners over the minutes measured (gaps left out) | no |
| **Listening hours** (TLH, total listening hours) | Sum of the concurrent listeners of each minute measured, divided by 60 | no |
| **Sessions** | Listening sessions of at least the minimum length (1 min by default) that started in the period | yes |
| **Average session** | Mean length of those sessions | yes |
| **Distinct listeners** | Listeners with at least one session of the minimum length in the period | yes |
| **Departures** of a track | Sessions of the minimum length that ended while the track was on air | yes |

A **session** is one listener's connection to one output: it starts when the server says the connection started (it tells how long each listener has been connected, to the second) and ends at the last minute the listener was seen, so its end is known to the minute. A listener who reconnects within a minute (same address and player, on the same output) continues the same session. Sessions shorter than the minimum length are kept but not counted: stream directories, monitoring probes and players that check a stream before playing it make many of them, and audience measurers leave them out too. Change the minimum in the statistics settings.

A **listener** is told apart from others, within a day, by their address and player (see [Privacy](#privacy)). People sharing an address and a player (a household, an office behind one router) count as one; one person on two devices counts as two.

### Days, hours and weeks

Days and hours are in the **station's time zone** (the `station` settings, see [Rotation](rotation.md#scheduling)). When the clocks go back, the repeated hour gathers both hours' minutes; when they go forward, the skipped hour has none. A session going on at midnight is split there: it ends the day and starts the next one, so each day counts its own listeners.

Distinct listeners cannot be added up across days: a listener is not recognized from one day to the next, by design. A week or a month shows the **average distinct listeners per day** (and their sum, *listener-days*), with the peak, listening hours and sessions of the whole period.

### Whole station

The station's figures add up the outputs minute by minute: its peak is the highest sum, not the sum of the outputs' peaks. A listener on two outputs at once counts once in its distinct listeners.

### Per track

For each play of the music decks (not the FX player), from the [play history](api.md#play-history): the station's listeners when it started and when it ended, the change, and its departures. Over a period, tracks are compared by their **departure rate**: departures over listeners at the start, so that a track played at a busy hour does not look worse than one played at night. Tracks played fewer than 3 times in the period (`min_plays`) are left out of that ranking.

Departures are counted to the minute: a session ends when its listener was last seen, so they left within the next minute, half a minute later on average, and that is when the departure is counted. A listener who leaves in the first seconds of a track may be counted on the one before. A session split at midnight is not a departure. With many plays the error averages out; a single play says little.

## Privacy

- **IP addresses are never stored**, not even for a minute. A listener is identified by a **listener key**: a BLAKE3 hash of their address and player, keyed with a random **daily key**. The daily key is kept in the database while its day can still get sessions, then deleted, so keys of different days cannot be linked, and a key cannot be traced back to an address by hashing every possible one.
- The player (user agent) is stored with each session: it tells which apps and devices listen.
- **Sessions are kept 90 days** (the statistics settings), the minute samples too. Daily and hourly totals are kept as long as the station keeps its database; delete them with SQL if needed.
- The admin password of a stream server is stored in the database with the output, like the source password, and never sent back by the API.

## Settings

`GET`, `PUT /api/v1/settings/statistics` (admins), also on the **Settings** page:

| Setting | Default | Meaning |
|---|---|---|
| `min_session_s` | 60 | Shortest session that counts as a listener, in seconds (0 counts every one) |
| `retention_days` | 90 | How long sessions and minute samples are kept (7 to 3650) |

Changing the minimum session length recomputes the totals still covered by the sessions kept.

Per output, the **statistics** settings (`outputs.statistics`, never pushed to the engine, so changing them does not reconnect the output):

| Setting | Default | Meaning |
|---|---|---|
| `admin_username` | `admin` | The stream server's admin user |
| `admin_password` | none | Its password: without it, Heron only counts concurrent listeners |

## API

Viewers can read every statistic; the routes are under `/api/v1/listeners`. Periods are dates (`YYYY-MM-DD`, station time, both included) or Unix milliseconds for minute samples; `output_id` limits the answer to one output (the whole station otherwise).

| Route | Answer |
|---|---|
| `GET /listeners/now` | Each enabled output's listeners at the last minute, when that was, whether its sessions are available, and the error if its server could not be read; the station's total |
| `GET /listeners/samples` (`from_unix_ms`, `to_unix_ms`, at most 8 days; `output_id`) | Concurrent listeners minute by minute, gaps as missing minutes |
| `GET /listeners/days` (`from`, `to`, `output_id`, `group`: `hour`, `day` (by default), `week`, `month`) | Per period: days and minutes measured, peak, average, listening hours, sessions, average session, distinct listeners (for weeks and months, the average per day) and listener-days |
| `GET /listeners/hours-of-day` (`from`, `to`, `output_id`) | For each hour of the day (0 to 23), averaged over the days of the period: average and peak listeners, listening hours |
| `GET /listeners/tracks` (`from`, `to`, `min_plays` (3), `limit`) | Tracks of the period by departure rate: plays, average listeners at start and end, departures, departure rate |
| `GET /listeners/players` (`from`, `to`, `output_id`, `limit`) | Sessions and listening hours by player (user agent) |

`days`, `hours-of-day` and `tracks` answer in CSV with `format=csv` (UTF-8, comma-separated, a header row, one row per day, hour or track), for reporting to rights societies or advertisers. Values not available (no sessions without admin credentials, a day with no minute measured) are empty in CSV and `null` in JSON.

Today's figures are computed as the minutes go by: today's row in `GET /listeners/days` changes until midnight.

## Web UI

The **Statistics** page: today live (listeners minute by minute, now, peak and listening hours so far), then by day, week or month over a period, the hours of the day, per output or for the whole station, the players, and the tracks with the most departures; each table can be downloaded in CSV. **On air** shows each output's current listeners, and the station's total.

## Metrics

`heron-api` exports `heron_listeners{output}`, each enabled output's listeners at the last minute (`NaN`, a gap in graphs, while its server cannot be read), and `heron_api_listener_polls_total{result}` (`ok`, `error`). The Grafana dashboard shows them in its **On air** row. See [Monitoring](monitoring.md#heron-api).

## Storage

See [Data model – Listener statistics](data-model.md#listener-statistics). Every minute, the collector writes the minute's samples, opens, continues and closes sessions, then recomputes the totals of the current hour and day (and of the previous ones, whose sessions may just have ended). The totals tables are small: one row per output and per hour.

## Limitations

- Only Icecast for now: Shoutcast with its outputs ([M11](roadmap/m11-sinks-live-dj.md)), HLS later.
- A minute's sample is the count at the moment of the request: a listener who stays less than a minute may be missed by the concurrent counts (sessions still see them, with admin credentials, when their connection lasts across a request).
- Statistics start when the API first reads the server; nothing is recovered for the time the API was down.
- Listeners going through a relay or a CDN in front of Icecast all appear with the relay's address: they are counted, but a reconnecting listener behind it cannot be told apart from another one with the same player.
