# Monitoring

How to watch a Heron station: its metrics in **Prometheus and Grafana**, alerts when something goes wrong, health checks, logs, and error reports in **Sentry**.

What Heron provides:

| | Where | Details |
|---|---|---|
| Metrics | `/metrics` on every Rust service (Prometheus text format) | [Metrics reference](#metrics-reference) |
| Health | `/healthz` on every service, used by Docker's health checks | [Health checks](#health-checks) |
| Logs | Standard output of each container, one JSON object per line | [Logs](#logs) |
| Errors | Sentry, when `HERON_SENTRY_DSN` is set | [Sentry](#sentry) |
| Ready-made monitoring | Prometheus, Alertmanager and Grafana with a Heron dashboard and alert rules: `docker compose --profile monitoring` | [Quick start](#quick-start-the-bundled-stack) |

## Quick start: the bundled stack

The Compose file has an optional `monitoring` profile. It adds Prometheus (scrapes every Heron service), Alertmanager (receives the alerts) and Grafana (dashboard), all configured from [`monitoring/`](../monitoring).

1. Set a password for Grafana's `admin` user in `.env` (see `.env.example`):

   ```sh
   HERON_GRAFANA_ADMIN_PASSWORD=a-long-password
   HERON_GRAFANA_PORT=3000        # optional
   HERON_METRICS_RETENTION=30d    # optional: how long Prometheus keeps metrics
   ```

2. Start the stack with the profile:

   ```sh
   docker compose --profile monitoring up -d
   ```

   Add `--profile icecast` too when you use the local Icecast server.

3. Open http://localhost:3000/ and sign in as `admin`. The **Heron Broadcaster** dashboard is the home page, also under *Dashboards → Heron*. Alerts are under *Alerting → Alert rules*, in the *Prometheus* data source.

Nothing else is published: Prometheus (9090) and Alertmanager (9093) are only reachable inside the Compose network, through Grafana. To open them from the host, see [Publishing the ports](#publishing-the-metrics-ports).

The files:

| File | Contents |
|---|---|
| `monitoring/prometheus/prometheus.yml` | What Prometheus scrapes (the engine every 5 s, the others every 15 s) |
| `monitoring/prometheus/alerts.yml` | [Alert rules](#alerts) |
| `monitoring/alertmanager/alertmanager.yml` | Where alerts go: nowhere until you add a [receiver](#notifications) |
| `monitoring/grafana/provisioning/` | Grafana's data sources (Prometheus, Alertmanager) and dashboard folder |
| `monitoring/grafana/dashboards/heron.json` | The [dashboard](#the-dashboard) |

Configuration changes need a restart of the service concerned (`docker compose --profile monitoring restart prometheus`). A dashboard edited in Grafana is kept until Grafana restarts: to keep it, export it (*Share → Export*) over `heron.json`, or save it under another name.

> On a Raspberry Pi, the monitoring stack takes about 300 MB of memory and some CPU every scrape. The engine comes first: on a small Pi, run Prometheus and Grafana on another machine and scrape the station from there (next section).

## Using your own Prometheus and Grafana

### Scraping the services

Each Rust service serves Prometheus metrics on `/metrics` and its health on `/healthz`, on its own port:

| Service | Port | Scrape interval |
|---|---|---|
| `heron-engine` | 8081 | 5 s (the air depends on it; short outages show) |
| `heron-api` | 8080 | 15 s |
| `heron-analyzer` | 8082 | 15 s |
| `heron-librarian` | 8083 | 15 s |

There is no authentication on these ports: keep them inside the Compose network, or publish them on `127.0.0.1` or a private network only.

**Prometheus in the same Compose project or on the same Docker network.** Scrape the services by name. Use the job names below: the dashboard and the alert rules expect them.

```yaml
scrape_configs:
  - job_name: heron-engine
    scrape_interval: 5s
    static_configs:
      - targets: ["heron-engine:8081"]
  - job_name: heron-api
    static_configs:
      - targets: ["heron-api:8080"]
  - job_name: heron-analyzer
    static_configs:
      - targets: ["heron-analyzer:8082"]
  - job_name: heron-librarian
    static_configs:
      - targets: ["heron-librarian:8083"]
```

A Prometheus running in another Compose project can join Heron's network (named `heron_default`):

```yaml
# In the other project's docker-compose.yml
services:
  prometheus:
    networks: [default, heron]
networks:
  heron:
    name: heron_default
    external: true
```

#### Publishing the metrics ports

**Prometheus on the host or on another machine.** Publish the ports with a `docker-compose.override.yml` next to `docker-compose.yml` (Compose reads it automatically). Bind them to `127.0.0.1`, or to the address of a private network (VPN, LAN), never to the Internet:

```yaml
services:
  heron-engine:
    ports: ["127.0.0.1:9181:8081"]
  heron-api:
    ports: ["127.0.0.1:9180:8080"]
  heron-analyzer:
    ports: ["127.0.0.1:9182:8082"]
  heron-librarian:
    ports: ["127.0.0.1:9183:8083"]
  # With the monitoring profile: Prometheus' and Alertmanager's own pages.
  prometheus:
    ports: ["127.0.0.1:9090:9090"]
  alertmanager:
    ports: ["127.0.0.1:9093:9093"]
```

Then scrape `127.0.0.1:9181` and so on, keeping the job names (`job_name: heron-engine`…).

Several stations in one Prometheus or Grafana: give each a `station` label, with `external_labels` in its Prometheus (commented out in `monitoring/prometheus/prometheus.yml`), or with `labels` on its targets:

```yaml
    static_configs:
      - targets: ["radio1.lan:9181"]
        labels: { station: radio1 }
```

### Grafana Cloud (or any remote Prometheus)

Run [Grafana Alloy](https://grafana.com/docs/alloy/latest/) next to the station, on Heron's network, to scrape the services and push the metrics with `remote_write`. A `config.alloy`:

```alloy
prometheus.scrape "heron_engine" {
  job_name        = "heron-engine"
  targets         = [{"__address__" = "heron-engine:8081"}]
  scrape_interval = "5s"
  forward_to      = [prometheus.remote_write.cloud.receiver]
}

prometheus.scrape "heron_services" {
  // One job per service: the dashboard and the alerts use the job names.
  job_name   = "heron-api"
  targets    = [{"__address__" = "heron-api:8080"}]
  forward_to = [prometheus.remote_write.cloud.receiver]
}
// …the same for heron-analyzer (8082) and heron-librarian (8083).

prometheus.remote_write "cloud" {
  external_labels = { station = "my-station" }
  endpoint {
    // From your Grafana Cloud stack: Prometheus → Details.
    url = "https://prometheus-prod-XX-prod-REGION.grafana.net/api/prom/push"
    basic_auth {
      username = "123456"
      password = sys.env("GRAFANA_CLOUD_TOKEN")
    }
  }
}
```

```yaml
# docker-compose.override.yml
services:
  alloy:
    image: grafana/alloy:v1.11.0
    restart: unless-stopped
    command: ["run", "/etc/alloy/config.alloy", "--storage.path=/var/lib/alloy"]
    environment:
      GRAFANA_CLOUD_TOKEN: ${GRAFANA_CLOUD_TOKEN}
    volumes:
      - ./config.alloy:/etc/alloy/config.alloy:ro
```

A self-hosted Prometheus can push the same way with its own `remote_write` section.

### Importing the dashboard

1. In Grafana: *Dashboards → New → Import*.
2. Upload `monitoring/grafana/dashboards/heron.json` (or paste its content).
3. Pick your Prometheus data source when asked, or later in the *Prometheus* drop-down at the top of the dashboard: every panel follows it.

The dashboard's UID is `heron-station`; importing it again replaces it.

### Loading the alert rules

- **Prometheus:** copy `monitoring/prometheus/alerts.yml` next to your configuration and list it under `rule_files`. Check it with `promtool check rules alerts.yml`. Alerts go to the Alertmanager configured under `alerting`.
- **Grafana Cloud or Mimir:** upload the file with [`mimirtool rules load alerts.yml`](https://grafana.com/docs/mimir/latest/manage/tools/mimirtool/), or recreate the rules you want as Grafana-managed alerts (*Alerting → Alert rules → New*), using the expressions of the file.

## The dashboard

Rows, from top to bottom:

| Row | What it shows |
|---|---|
| **On air** | Whether the engine answers, dead air, how long the master bus has been silent, outputs connected, tracks queued, whether the server clock is synchronized. Master level, each output's bitrate, fallback tracks and silence watchdog skips, pause and queue length over time. |
| **Audio engine health** | Underruns (decoding fell behind), audio dropped by the engine, an encoder or an output, output reconnections, the pacing clock's lateness and drift, stalls, tracks that could not be preloaded. |
| **Server clock** | The kernel's clock offset, estimated error and synchronization ([Requirements – Reliability](requirements.md#5-reliability-247)). |
| **Control plane** | heron-api, its link to the engine, web clients connected, the analyzer and the librarian, library size, tracks queued by rotation and picks with relaxed rules, sign-ins. |
| **Library** | Analysis jobs by result and time per track, imports and library scans. |
| **Versions** | The version of every running service (`heron_build_info`). |

What healthy looks like: *Dead air* OK, *Silence* under a few seconds (pauses aside), every output connected, *Queue* 2 or more, *Clock drift* flat at 50 ms, underruns and dropped audio at zero, fallback tracks only when you expect them.

## Alerts

Rules in `monitoring/prometheus/alerts.yml`. **Critical**: listeners hear the problem, or soon will. **Warning**: the air is safe, something needs a look.

| Alert | Severity | Fires when | What to do |
|---|---|---|---|
| `HeronEngineDown` | critical | heron-engine does not answer for 1 min | `docker compose ps heron-engine`, `docker compose logs heron-engine`. The engine exits by itself when its audio thread is stuck and Docker restarts it: repeated restarts point at the machine. |
| `HeronDeadAir` | critical | Nothing on air and nothing to play for 30 s | Put tracks in the fallback directory (`HERON_FALLBACK_DIR`). Check heron-api and that rotation can pick tracks (Rotation page). |
| `HeronSilenceOnAir` | critical | The master bus is silent for 45 s, not paused | The silence watchdog is off, or every queued and fallback track is silent. |
| `HeronOutputDisconnected` | critical | An output is not connected for 2 min | Check the stream server and the output's host, mount and password (Outputs page). The engine keeps retrying. |
| `HeronAudioDropped` | critical | The engine dropped audio in the last 10 min | The machine is too busy for the encoders: fewer or cheaper encoder profiles, or a faster machine. |
| `HeronFallbackPlaying` | warning | A fallback track started in the last 10 min | The queue ran dry: heron-api down or cut from the engine, autofill off, or restrictions forbid every track. |
| `HeronSilenceSkips` | warning | The watchdog skipped a track in the last 30 min | The engine's logs name the track (`silence on air`): check its file and its analysis. |
| `HeronUnderruns` | warning | More than 50 blocks without decoded audio in 10 min | The machine is overloaded, or the library is slow (network share). |
| `HeronOutputUnstable` | warning | An output reconnected more than 3 times in 30 min | Network or stream server trouble. |
| `HeronPreloadFailures` | warning | Queued tracks could not be read | The library is unreachable, or files were deleted. |
| `HeronClockStalled` | warning | The engine stalled for more than 1 s in the last hour | The machine was suspended or starved of CPU: listeners heard a gap. |
| `HeronClockNotSynchronized` | warning | The kernel reports the clock unsynchronized for 10 min | Run NTP on the host (chrony, systemd-timesyncd). |
| `HeronClockOff` | warning | The clock may be off by more than 1 s for 10 min | Same. |
| `HeronServiceDown` | warning | heron-api, heron-analyzer or heron-librarian does not answer for 2 min | The air stays on; the station cannot be controlled or fed. |
| `HeronApiLostEngine` | warning | heron-api cannot reach the engine for 2 min | The queue is not refilled; the fallback directory takes over when it runs dry. |
| `HeronAnalysisFailing` | warning | More than 10 analyses failed in an hour | Library page, heron-analyzer's logs. |
| `HeronLibraryScanFailing` | warning | A library scan failed in the last 2 h | Is the music directory mounted and readable? |
| `HeronLoginFailures` | warning | More than 20 failed sign-ins in 10 min | Someone may be guessing passwords. |

Thresholds are starting points: edit them in `alerts.yml` to suit the station.

### Notifications

Out of the box, alerts are only shown (Grafana's alert list). Add a receiver to `monitoring/alertmanager/alertmanager.yml` and make it the route's `receiver`, then `docker compose --profile monitoring restart alertmanager`. Check the file with `docker compose --profile monitoring exec alertmanager amtool check-config /etc/alertmanager/alertmanager.yml`.

Email:

```yaml
route:
  receiver: team
receivers:
  - name: team
    email_configs:
      - to: oncall@example.org
        from: heron@example.org
        smarthost: smtp.example.org:587
        auth_username: heron@example.org
        auth_password: "…"
```

Discord, Slack, Telegram, Microsoft Teams (one or several in the same receiver):

```yaml
receivers:
  - name: team
    discord_configs:
      - webhook_url: https://discord.com/api/webhooks/…
    slack_configs:
      - api_url: https://hooks.slack.com/services/…
        channel: "#radio"
    telegram_configs:
      - bot_token: "…"
        chat_id: -1001234567890
    msteamsv2_configs:
      - webhook_url: https://…
```

Critical alerts only to a phone, everything to email:

```yaml
route:
  receiver: email
  routes:
    - matchers: ['severity="critical"']
      receiver: phone
      continue: true
```

See [Alertmanager's configuration](https://prometheus.io/docs/alerting/latest/configuration/) for the other receivers (PagerDuty, Opsgenie, webhooks…).

## Metrics reference

Every service also exports `heron_build_info{service, version}` (always 1).

### heron-engine

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `heron_engine_rendered_seconds` | gauge | | Audio rendered since the engine started |
| `heron_engine_clock_drift_seconds` | gauge | | Audio rendered minus time elapsed: constant (the 50 ms lead) while the clock keeps time |
| `heron_engine_clock_max_late_seconds` | gauge | | Worst lateness of a 10 ms block over the last second |
| `heron_engine_clock_resets_total` | counter | | Stalls longer than 1 s (machine suspended, CPU starved) |
| `heron_engine_underruns_total` | counter | | 10 ms blocks where a deck had no decoded audio |
| `heron_engine_underrun_tracks_total` | counter | | Tracks that had at least one underrun |
| `heron_engine_dropped_blocks_total` | counter | | 10 ms blocks the encoders did not take in time (lost audio) |
| `heron_engine_mixer_overflows_total` | counter | | Mixer events the player could not take in time (never expected) |
| `heron_engine_paused` | gauge | | 1 while paused |
| `heron_engine_master_peak` | gauge | | Peak level of the master bus, linear (1 = full scale); in dBFS: `20 * log10(...)` |
| `heron_engine_silent_seconds` | gauge | | How long the master bus has been below −50 dBFS (0 when there is sound) |
| `heron_engine_queue_length` | gauge | | Tracks waiting after the cued one |
| `heron_engine_dead_air` | gauge | | 1 when nothing is on air and there is nothing to play |
| `heron_engine_fallback_tracks_total` | counter | | Tracks started from the fallback directory |
| `heron_engine_silence_skips_total` | counter | | Tracks skipped by the silence watchdog |
| `heron_engine_preload_failures_total` | counter | | Queued tracks whose file could not be read ahead of time |
| `heron_clock_synchronized` | gauge | | 1 when the kernel reports the clock synchronized (Linux only) |
| `heron_clock_offset_seconds` | gauge | | Offset the kernel is correcting |
| `heron_clock_estimated_error_seconds` | gauge | | The kernel's estimate of the clock's error |
| `heron_output_connected` | gauge | `output` | 1 while the output streams |
| `heron_output_bytes_total` | counter | `output` | Bytes sent to the stream server |
| `heron_output_reconnects_total` | counter | `output` | Reconnections |
| `heron_output_dropped_chunks_total` | counter | `output` | Encoded audio dropped because the output was behind |
| `heron_encoder_dropped_chunks_total` | counter | `profile` | Audio dropped because the encoder was behind |

### heron-api

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `heron_api_engine_connected` | gauge | | 1 while the API is connected to the engine |
| `heron_api_engine_disconnections_total` | counter | | Connections to the engine lost |
| `heron_api_live_clients` | gauge | | Web clients on the live WebSocket |
| `heron_api_autofill_tracks_total` | counter | | Tracks queued by autofill (rotation) |
| `heron_api_rotation_relaxed_total` | counter | | Picks that needed relaxed rules |
| `heron_api_logins_total` | counter | `result` (`ok`, `failed`) | Sign-ins |
| `heron_library_files` | gauge | | Audio files found by the last library scan |
| `heron_library_scan_duration_seconds` | gauge | | Duration of the last scan |
| `heron_library_scans_total` | counter | `result` (`ok`, `error`) | Library scans |

### heron-analyzer

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `heron_analysis_jobs_total` | counter | `result` (`ok`, `retry`, `failed`, `error`) | Analysis jobs finished |
| `heron_analysis_seconds` | summary | `quantile` | Time to analyze a track |
| `heron_analysis_audio_ms_total` | counter | | Audio analyzed, in ms |

### heron-librarian

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `heron_librarian_imports_total` | counter | `result` (`identified`, `needs_review`, `unidentified`, `failed`) | Files imported from the incoming folder |

Counters that only grow when something happens (`heron_engine_silence_skips_total`…) start at 0 when the engine starts, so `increase()` works from the first scrape.

## Health checks

Every service answers `GET /healthz` on its port:

```json
{"service": "heron-engine", "version": "0.1.0", "status": "ok"}
```

- **200** with `"status": "ok"`: healthy. Docker's health checks (in the images) use it; `docker compose ps` shows the result.
- **503** with `"status": "unavailable"` and a `reason`: unhealthy (the engine's audio thread stopped rendering, the API lost its database…).
- **`warnings`**: problems that need attention without making the service unhealthy. The engine lists:
  - `the server clock is not synchronized (is NTP running on the host?)` or `the server clock may be off by 2.5 s`;
  - `nothing to play: the queue is empty and the fallback directory has no track`.

Every service restarts by itself (`restart: unless-stopped`). Docker does not restart a container only because it is unhealthy, so the engine exits on its own when its audio thread is stuck for 10 s: Docker then restarts it, and it resumes the queue it saved.

From outside, an uptime monitor ([Uptime Kuma](https://github.com/louislam/uptime-kuma), Grafana Synthetic Monitoring, UptimeRobot…) should watch what listeners get: the stream URL itself (an HTTP check of `https://stream.example.org/live` expecting `200`, or Prometheus' [blackbox exporter](https://github.com/prometheus/blackbox_exporter)). It catches what the station cannot see from inside: DNS, TLS, the network, the stream server.

## Logs

Containers log one JSON object per line (`HERON_LOG_FORMAT=json`), with `timestamp`, `level`, `target`, `message` and the event's fields:

```json
{"timestamp":"2026-10-18T19:26:47.99Z","level":"WARN","message":"silence on air: skipping the track","entry_id":42,"path":"\"Artist/Album/01 Track.mp3\"","seconds":20,"to_fallback":false,"target":"heron_engine::player"}
```

The level is set with `RUST_LOG` in `.env` (`info` by default; `heron_engine=debug,info` for more from the engine).

`docker compose logs -f heron-engine` follows one service. To search them in Grafana, ship them to [Loki](https://grafana.com/docs/loki/latest/) with Grafana Alloy:

```alloy
discovery.docker "containers" {
  host = "unix:///var/run/docker.sock"
}

discovery.relabel "heron" {
  targets = discovery.docker.containers.targets
  rule {
    source_labels = ["__meta_docker_container_label_com_docker_compose_service"]
    target_label  = "service"
  }
}

loki.source.docker "heron" {
  host       = "unix:///var/run/docker.sock"
  targets    = discovery.relabel.heron.output
  forward_to = [loki.write.default.receiver]
}

loki.write "default" {
  endpoint {
    url = "http://loki:3100/loki/api/v1/push"
  }
}
```

(mount `/var/run/docker.sock` read-only into the Alloy container). Useful queries:

```logql
{service="heron-engine"} | json | level="ERROR"
{service="heron-engine"} |= "silence on air"
{service=~"heron-.*"} | json | level=~"WARN|ERROR"
```

## Sentry

Errors and panics of every service, and JavaScript errors of the web interface, can be reported to [Sentry](https://sentry.io) or a self-hosted Sentry. It is off unless a DSN is set.

| Variable | Used by | Meaning |
|---|---|---|
| `HERON_SENTRY_DSN` | Rust services | DSN of the project receiving the services' errors |
| `HERON_WEB_SENTRY_DSN` | `heron-web` | DSN of the project receiving the browser's errors |
| `HERON_SENTRY_ENVIRONMENT` | both | Environment shown in Sentry (`production` by default) |
| `HERON_SENTRY_TRACES_SAMPLE_RATE` | Rust services | Share of operations traced for performance monitoring, 0 (default) to 1 |

1. In Sentry, create a project for the services (platform *Rust*) and, if you want browser errors, another for the web interface (platform *React*). Browsers can read the web DSN, so keep it separate from the services'.
2. Copy their DSNs into `.env`:

   ```sh
   HERON_SENTRY_DSN=https://<key>@o123456.ingest.sentry.io/1234567
   HERON_WEB_SENTRY_DSN=https://<key>@o123456.ingest.sentry.io/7654321
   HERON_SENTRY_ENVIRONMENT=production
   ```

3. `docker compose up -d`. Each service logs `reporting errors to Sentry` at startup.

What is sent:

- **Rust services:** every `ERROR` log becomes a Sentry event, with the `INFO` and `WARN` logs before it as breadcrumbs, and every panic. Events carry the release (`heron-engine@0.1.0`…), a `service` tag and the host's name. No IP addresses or request bodies. The engine's audio thread never logs, so reporting never touches it.
- **Web interface:** uncaught errors and unhandled promise rejections, with the release `heron-web@<version>` and a `service` tag. No user details, cookies, headers, request bodies or query strings (sessions and API tokens travel in them). The Sentry SDK is only downloaded by browsers when the DSN is set.

The web interface reads its DSN when the page loads, from `/config.js`, which `heron-web` builds from its environment at startup: changing it needs `docker compose up -d heron-web`, not a rebuild.

## What else could be monitored

Not provided yet, and worth adding for a station on air around the clock:

- **Listeners**: listener counts per output and per track, from the stream servers ([M7b](roadmap/m7b-listener-statistics.md)). Until then, an [Icecast exporter](https://github.com/markuslindenberg/icecast_exporter) gives the current count to Prometheus.
- **The host**: CPU, memory, disk, temperature (a Raspberry Pi throttles when hot) with [node_exporter](https://github.com/prometheus/node_exporter), and per-container use with [cAdvisor](https://github.com/google/cadvisor).
- **MariaDB**: with [mysqld_exporter](https://github.com/prometheus/mysqld_exporter).
- **The stream from outside**: see [Health checks](#health-checks).
- **Track changes as events**: webhooks on track start, end and skip ([M7](roadmap/m7-webhooks-output-api.md)) can post to a chat or annotate Grafana.

## Troubleshooting

| Symptom | Cause |
|---|---|
| A target is *down* in Prometheus (*Status → Targets*) | The service is down, or Prometheus is not on its network: `docker compose --profile monitoring ps`. |
| Dashboard panels say *No data* | Pick the right data source at the top of the dashboard. Output panels stay empty until an output is configured. |
| Clock panels are empty | The clock state is read from the Linux kernel (in containers, the host's or Docker Desktop's VM's): nothing when the engine runs natively on macOS or Windows. |
| No alerts in Grafana | *Alerting → Alert rules* lists them under the Prometheus data source; check `docker compose --profile monitoring logs prometheus` for rule errors. |
| No notification | A receiver must be added to `alertmanager.yml` (none out of the box). |
| Nothing in Sentry | Check the DSN, and that the service logged `reporting errors to Sentry`; the container needs outgoing HTTPS. |
