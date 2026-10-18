// SPDX-License-Identifier: AGPL-3.0-or-later

//! Pushes the service's metrics to Prometheus with remote write, so a
//! station needs no Prometheus of its own: Grafana Cloud, Mimir,
//! VictoriaMetrics or a Prometheus started with
//! `--web.enable-remote-write-receiver` receive them directly.
//!
//! Every `HERON_METRICS_REMOTE_WRITE_INTERVAL` seconds, the metrics served
//! on `/metrics` are sent as one remote write 1.0 request (protobuf,
//! snappy), labelled with the service (`job`) and the station (`station`),
//! the labels the dashboard and the alert rules use. A failed push is not
//! retried: counters are totals, so the next push carries what was missed.

use std::time::{Duration, SystemTime};

use metrics_exporter_prometheus::PrometheusHandle;
use prost::Message;

use crate::Error;

const DEFAULT_INTERVAL: Duration = Duration::from_secs(15);

/// Remote write options shared by every binary.
#[derive(Clone, clap::Args)]
pub struct MetricsArgs {
    /// Prometheus remote write endpoint the metrics are pushed to (Grafana
    /// Cloud: `https://…grafana.net/api/prom/push`). Off when empty.
    #[arg(long, env = "HERON_METRICS_REMOTE_WRITE_URL", global = true)]
    pub remote_write_url: Option<String>,

    /// User for basic authentication (Grafana Cloud: the instance ID).
    /// Without one, the password is sent as a bearer token.
    #[arg(long, env = "HERON_METRICS_REMOTE_WRITE_USERNAME", global = true)]
    pub remote_write_username: Option<String>,

    /// Password or token (Grafana Cloud: an access policy token with the
    /// metrics:write scope).
    #[arg(
        long,
        env = "HERON_METRICS_REMOTE_WRITE_PASSWORD",
        global = true,
        hide_env_values = true
    )]
    pub remote_write_password: Option<String>,

    /// Seconds between pushes. Read leniently: an invalid value means 15.
    #[arg(
        long,
        env = "HERON_METRICS_REMOTE_WRITE_INTERVAL",
        default_value = "15",
        global = true
    )]
    pub remote_write_interval: String,

    /// Name of the station: the `station` label of the metrics pushed.
    #[arg(
        long,
        env = "HERON_STATION_NAME",
        default_value = "heron",
        global = true
    )]
    pub station_name: String,
}

// Keeps the password out of logs.
impl std::fmt::Debug for MetricsArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsArgs")
            .field("remote_write_url", &self.remote_write_url)
            .field("remote_write_username", &self.remote_write_username)
            .field(
                "remote_write_password",
                &self.remote_write_password.as_ref().map(|_| "<set>"),
            )
            .field("remote_write_interval", &self.remote_write_interval)
            .field("station_name", &self.station_name)
            .finish()
    }
}

// Remote write 1.0 messages (prometheus/prompb/remote.proto, types.proto).

#[derive(Clone, PartialEq, prost::Message)]
pub struct WriteRequest {
    #[prost(message, repeated, tag = "1")]
    pub timeseries: Vec<TimeSeries>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct TimeSeries {
    #[prost(message, repeated, tag = "1")]
    pub labels: Vec<Label>,
    #[prost(message, repeated, tag = "2")]
    pub samples: Vec<Sample>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Label {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Sample {
    #[prost(double, tag = "1")]
    pub value: f64,
    /// Milliseconds since the Unix epoch.
    #[prost(int64, tag = "2")]
    pub timestamp: i64,
}

/// Starts pushing the metrics of `handle` when a remote write URL is set.
pub fn spawn(
    handle: PrometheusHandle,
    service: &'static str,
    args: &MetricsArgs,
) -> Result<(), Error> {
    let Some(url) = args
        .remote_write_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    else {
        return Ok(());
    };
    let url = reqwest::Url::parse(url)
        .map_err(|error| Error::RemoteWrite(format!("invalid URL: {error}")))?;
    let host = url.host_str().unwrap_or_default().to_owned();
    let interval = match args.remote_write_interval.trim().parse::<u64>() {
        Ok(seconds) if seconds > 0 => Duration::from_secs(seconds),
        _ => {
            tracing::error!(
                value = %args.remote_write_interval,
                "invalid HERON_METRICS_REMOTE_WRITE_INTERVAL (whole seconds): using 15"
            );
            DEFAULT_INTERVAL
        }
    };
    let client = reqwest::Client::builder()
        .timeout(interval.min(Duration::from_secs(10)))
        .user_agent(concat!("heron/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| Error::RemoteWrite(error.to_string()))?;
    let pusher = Pusher {
        client,
        url,
        username: args.remote_write_username.clone().filter(|u| !u.is_empty()),
        password: args.remote_write_password.clone().filter(|p| !p.is_empty()),
        labels: vec![
            ("job".to_owned(), service.to_owned()),
            ("station".to_owned(), args.station_name.clone()),
        ],
    };
    tracing::info!(%host, seconds = interval.as_secs(), "pushing metrics with remote write");
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(interval);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut failing = false;
        loop {
            ticks.tick().await;
            match pusher.push(&handle.render()).await {
                Ok(()) => {
                    if failing {
                        tracing::info!(%host, "pushing metrics again");
                    }
                    failing = false;
                }
                Err(error) => {
                    metrics::counter!("heron_remote_write_failures_total").increment(1);
                    // Once per outage, not every interval.
                    if !failing {
                        tracing::warn!(%host, %error, "cannot push metrics");
                    }
                    failing = true;
                }
            }
        }
    });
    Ok(())
}

struct Pusher {
    client: reqwest::Client,
    url: reqwest::Url,
    username: Option<String>,
    password: Option<String>,
    /// Added to every series (`job`, `station`).
    labels: Vec<(String, String)>,
}

impl Pusher {
    async fn push(&self, exposition: &str) -> Result<(), String> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as i64);
        let request = write_request(exposition, &self.labels, timestamp);
        let body = snap::raw::Encoder::new()
            .compress_vec(&request.encode_to_vec())
            .map_err(|error| error.to_string())?;
        let mut post = self
            .client
            .post(self.url.clone())
            .header("Content-Type", "application/x-protobuf")
            .header("Content-Encoding", "snappy")
            .header("X-Prometheus-Remote-Write-Version", "0.1.0")
            .body(body);
        post = match (&self.username, &self.password) {
            (Some(username), password) => post.basic_auth(username, password.as_ref()),
            (None, Some(token)) => post.bearer_auth(token),
            (None, None) => post,
        };
        let response = post.send().await.map_err(|error| error.to_string())?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        Err(format!(
            "{status}: {}",
            text.trim().chars().take(300).collect::<String>()
        ))
    }
}

/// The samples of a Prometheus text exposition as a remote write request,
/// with `extra` labels on every series.
pub fn write_request(exposition: &str, extra: &[(String, String)], timestamp: i64) -> WriteRequest {
    let timeseries = exposition
        .lines()
        .filter_map(parse_line)
        .map(|(name, labels, value)| {
            let mut labels: Vec<Label> = std::iter::once(("__name__".to_owned(), name))
                .chain(
                    labels
                        .into_iter()
                        .filter(|(label, _)| extra.iter().all(|(name, _)| name != label)),
                )
                .chain(extra.iter().cloned())
                .map(|(name, value)| Label { name, value })
                .collect();
            // Remote write wants labels sorted by name.
            labels.sort_by(|a, b| a.name.cmp(&b.name));
            TimeSeries {
                labels,
                samples: vec![Sample { value, timestamp }],
            }
        })
        .collect();
    WriteRequest { timeseries }
}

type ParsedSample = (String, Vec<(String, String)>, f64);

/// One sample line of the text format: `name{label="value",…} value`.
/// `None` for comments, blank and malformed lines.
fn parse_line(line: &str) -> Option<ParsedSample> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let name_end = line.find(['{', ' ', '\t'])?;
    let name = line[..name_end].to_owned();
    let mut rest = &line[name_end..];
    let mut labels = Vec::new();
    if let Some(inner) = rest.strip_prefix('{') {
        let (parsed, after) = parse_labels(inner)?;
        labels = parsed;
        rest = after;
    }
    let value = rest.split_whitespace().next()?;
    let value = match value {
        "+Inf" => f64::INFINITY,
        "-Inf" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        value => value.parse().ok()?,
    };
    Some((name, labels, value))
}

/// Labels up to the closing `}`, and what follows it.
fn parse_labels(mut text: &str) -> Option<(Vec<(String, String)>, &str)> {
    let mut labels = Vec::new();
    loop {
        text = text.trim_start_matches([' ', ',']);
        if let Some(after) = text.strip_prefix('}') {
            return Some((labels, after));
        }
        let equals = text.find('=')?;
        let name = text[..equals].trim().to_owned();
        let quoted = text[equals + 1..].trim_start().strip_prefix('"')?;
        let mut value = String::new();
        let mut chars = quoted.char_indices();
        let end = loop {
            match chars.next()? {
                (index, '"') => break index,
                (_, '\\') => match chars.next()?.1 {
                    'n' => value.push('\n'),
                    other => value.push(other),
                },
                (_, other) => value.push(other),
            }
        };
        labels.push((name, value));
        text = &quoted[end + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPOSITION: &str = r#"# TYPE heron_output_connected gauge
heron_output_connected{output="Main \"FM\" stream"} 1

# TYPE heron_engine_rendered_seconds gauge
heron_engine_rendered_seconds 12.5
heron_analysis_seconds{quantile="0.5"} 0.25
heron_analysis_seconds_count 4
heron_odd{a="x,y}",job="ignored"} NaN
not a sample
"#;

    #[test]
    fn parses_the_text_format() {
        let samples: Vec<_> = EXPOSITION.lines().filter_map(parse_line).collect();
        assert_eq!(samples.len(), 5);
        assert_eq!(samples[0].0, "heron_output_connected");
        assert_eq!(
            samples[0].1,
            [("output".to_owned(), "Main \"FM\" stream".to_owned())]
        );
        assert_eq!(samples[0].2, 1.0);
        assert_eq!(samples[1].1, []);
        assert_eq!(samples[1].2, 12.5);
        assert_eq!(samples[4].1[0], ("a".to_owned(), "x,y}".to_owned()));
        assert!(samples[4].2.is_nan());
    }

    #[test]
    fn labels_every_series_with_the_service_and_station() {
        let extra = [
            ("job".to_owned(), "heron-engine".to_owned()),
            ("station".to_owned(), "radio1".to_owned()),
        ];
        let request = write_request(EXPOSITION, &extra, 1_000);
        assert_eq!(request.timeseries.len(), 5);
        let names = |series: &TimeSeries| -> Vec<String> {
            series
                .labels
                .iter()
                .map(|l| format!("{}={}", l.name, l.value))
                .collect()
        };
        assert_eq!(
            names(&request.timeseries[0]),
            [
                "__name__=heron_output_connected",
                "job=heron-engine",
                "output=Main \"FM\" stream",
                "station=radio1"
            ]
        );
        // Our job label wins over one in the exposition.
        assert_eq!(
            names(&request.timeseries[4]),
            [
                "__name__=heron_odd",
                "a=x,y}",
                "job=heron-engine",
                "station=radio1"
            ]
        );
        assert_eq!(request.timeseries[1].samples[0].timestamp, 1_000);
    }

    #[tokio::test]
    async fn pushes_snappy_protobuf_with_credentials() {
        use axum::body::Bytes;
        use axum::http::HeaderMap;

        let (received_tx, mut received) = tokio::sync::mpsc::channel(1);
        let app = axum::Router::new().route(
            "/push",
            axum::routing::post(move |headers: HeaderMap, body: Bytes| {
                let received_tx = received_tx.clone();
                async move {
                    let _ = received_tx.send((headers, body)).await;
                    ""
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });

        let pusher = Pusher {
            client: reqwest::Client::new(),
            url: format!("http://{addr}/push").parse().unwrap(),
            username: Some("123456".to_owned()),
            password: Some("secret".to_owned()),
            labels: vec![("job".to_owned(), "heron-api".to_owned())],
        };
        pusher.push(EXPOSITION).await.unwrap();
        let (headers, body) = received.recv().await.unwrap();
        assert_eq!(headers["content-encoding"], "snappy");
        assert_eq!(headers["x-prometheus-remote-write-version"], "0.1.0");
        // 123456:secret
        assert_eq!(headers["authorization"], "Basic MTIzNDU2OnNlY3JldA==");
        let protobuf = snap::raw::Decoder::new().decompress_vec(&body).unwrap();
        let request = WriteRequest::decode(protobuf.as_slice()).unwrap();
        assert_eq!(request.timeseries.len(), 5);
    }
}
