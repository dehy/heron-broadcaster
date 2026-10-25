// SPDX-License-Identifier: AGPL-3.0-or-later

//! Icecast source client: HTTP `PUT` to the mount (Icecast 2.4 and later),
//! metadata through `/admin/metadata`.

use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

use crate::ConfigError;
use crate::connection::{Connection, HandedConnection};
use crate::encoder::StreamInfo;
use crate::sink::{NowPlaying, OutputSink, SinkError};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A server that accepts no data for this long is considered gone.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_HEADER: usize = 16 * 1024;
const USER_AGENT: &str = concat!("HeronBroadcaster/", env!("CARGO_PKG_VERSION"));

/// `outputs.config` of an Icecast output.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcecastConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Mount point, starting with `/`.
    pub mount: String,
    #[serde(default = "default_username")]
    pub username: String,
    pub password: String,
    /// Station name, description, genre and website, shown in directories.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// List the stream in public directories.
    #[serde(default)]
    pub public: bool,
    /// Server address as listeners reach it, e.g. `https://stream.example.org`;
    /// defaults to `http://host:port`.
    #[serde(default)]
    pub public_url: Option<String>,
}

fn default_port() -> u16 {
    8000
}

fn default_username() -> String {
    "source".to_owned()
}

impl IcecastConfig {
    pub fn parse(output: &str, config: &serde_json::Value) -> Result<Self, ConfigError> {
        let invalid = |reason: String| ConfigError::Output {
            output: output.to_owned(),
            reason,
        };
        let mut config: Self = serde_json::from_value(config.clone())
            .map_err(|error| invalid(format!("invalid Icecast settings: {error}")))?;
        if !config.mount.starts_with('/') || config.mount.len() < 2 {
            return Err(invalid(format!(
                "the mount must start with / (got {:?})",
                config.mount
            )));
        }
        if config
            .mount
            .contains(|c: char| c.is_whitespace() || c.is_control() || c == '?')
        {
            return Err(invalid(format!("invalid mount {:?}", config.mount)));
        }
        if config.host.is_empty() {
            return Err(invalid("the host is missing".to_owned()));
        }
        config.public_url = config.public_url.filter(|url| !url.is_empty());
        if let Some(url) = &config.public_url
            && (!(url.starts_with("http://") || url.starts_with("https://"))
                || url.contains(|c: char| c.is_whitespace() || c.is_control()))
        {
            return Err(invalid(format!(
                "the public URL must start with http:// or https:// (got {url:?})"
            )));
        }
        Ok(config)
    }

    /// Where listeners can play the stream.
    pub fn listen_url(&self) -> String {
        match &self.public_url {
            Some(url) => format!("{}{}", url.trim_end_matches('/'), self.mount),
            None => format!("http://{}:{}{}", self.host, self.port, self.mount),
        }
    }
}

pub struct IcecastSink {
    config: IcecastConfig,
    info: StreamInfo,
    stream: Option<Connection>,
    /// A source connection handed over by another engine, streaming
    /// already: the first `connect` takes it instead of connecting.
    adopted: Option<HandedConnection>,
    /// The relay socket the source connection goes through, if any.
    relay: Option<PathBuf>,
}

impl IcecastSink {
    pub fn new(config: IcecastConfig, info: StreamInfo) -> Self {
        Self {
            config,
            info,
            stream: None,
            adopted: None,
            relay: None,
        }
    }

    /// Connects through the relay listening on `relay` (`heron-engine relay`).
    pub fn with_relay(mut self, relay: Option<PathBuf>) -> Self {
        self.relay = relay;
        self
    }

    /// A sink going on with `connection`, a source connection to the same
    /// server and mount that another engine was streaming to.
    pub fn adopt(config: IcecastConfig, info: StreamInfo, connection: HandedConnection) -> Self {
        Self {
            adopted: Some(connection),
            ..Self::new(config, info)
        }
    }

    fn authorization(&self) -> String {
        let credentials = format!("{}:{}", self.config.username, self.config.password);
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(credentials)
        )
    }

    fn source_request(&self) -> String {
        let config = &self.config;
        let mut audio_info = format!(
            "ice-samplerate={};ice-channels={}",
            self.info.sample_rate, self.info.channels
        );
        if let Some(bitrate) = self.info.bitrate_kbps {
            audio_info.push_str(&format!(";ice-bitrate={bitrate}"));
        }
        let mut request = format!(
            "PUT {mount} HTTP/1.1\r\n\
             Host: {host}:{port}\r\n\
             Authorization: {auth}\r\n\
             User-Agent: {USER_AGENT}\r\n\
             Content-Type: {content_type}\r\n\
             Expect: 100-continue\r\n\
             Ice-Public: {public}\r\n\
             Ice-Audio-Info: {audio_info}\r\n",
            mount = config.mount,
            host = config.host,
            port = config.port,
            auth = self.authorization(),
            content_type = self.info.content_type,
            public = u8::from(config.public),
        );
        if let Some(bitrate) = self.info.bitrate_kbps {
            request.push_str(&format!("Ice-Bitrate: {bitrate}\r\n"));
        }
        for (header, value) in [
            ("Ice-Name", &config.name),
            ("Ice-Description", &config.description),
            ("Ice-Genre", &config.genre),
            ("Ice-Url", &config.url),
        ] {
            if let Some(value) = value {
                request.push_str(&format!("{header}: {}\r\n", header_value(value)));
            }
        }
        request.push_str("\r\n");
        request
    }

    /// A connection for a metadata update: direct, it is short-lived.
    async fn open(&self) -> Result<Connection, SinkError> {
        Connection::open(&self.config.host, self.config.port, None).await
    }
}

impl OutputSink for IcecastSink {
    async fn connect(&mut self) -> Result<(), SinkError> {
        self.stream = None;
        if let Some(connection) = self.adopted.take() {
            self.stream = Some(Connection::adopt(connection)?);
            return Ok(());
        }
        let mut stream =
            Connection::open(&self.config.host, self.config.port, self.relay.as_deref()).await?;
        let request = self.source_request();
        timeout(WRITE_TIMEOUT, stream.write_all(request.as_bytes()))
            .await
            .map_err(|_| SinkError::Timeout("sending the request"))??;
        let status = timeout(CONNECT_TIMEOUT, read_status(&mut stream))
            .await
            .map_err(|_| SinkError::Timeout("waiting for the server's answer"))??;
        match status {
            // `100 Continue` when the server honors `Expect`, `200 OK` otherwise.
            (100 | 200, _) => {
                self.stream = Some(stream);
                Ok(())
            }
            (401, _) => Err(SinkError::Refused("wrong username or password".to_owned())),
            (403, line) => Err(SinkError::Refused(format!(
                "{line} (mount in use, or too many sources)"
            ))),
            (_, line) => Err(SinkError::Refused(line)),
        }
    }

    async fn send(&mut self, data: &[u8]) -> Result<(), SinkError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| SinkError::Io(std::io::Error::other("not connected")))?;
        timeout(WRITE_TIMEOUT, stream.write_all(data))
            .await
            .map_err(|_| SinkError::Timeout("sending audio"))??;
        Ok(())
    }

    async fn update_metadata(&mut self, now_playing: &NowPlaying) -> Result<(), SinkError> {
        let Some(title) = now_playing.stream_title() else {
            return Ok(());
        };
        let request = format!(
            "GET /admin/metadata?mount={mount}&mode=updinfo&charset=UTF-8&song={song} HTTP/1.0\r\n\
             Host: {host}:{port}\r\n\
             Authorization: {auth}\r\n\
             User-Agent: {USER_AGENT}\r\n\r\n",
            mount = utf8_percent_encode(&self.config.mount, NON_ALPHANUMERIC),
            song = utf8_percent_encode(&title, NON_ALPHANUMERIC),
            host = self.config.host,
            port = self.config.port,
            auth = self.authorization(),
        );
        let mut stream = self.open().await?;
        timeout(WRITE_TIMEOUT, stream.write_all(request.as_bytes()))
            .await
            .map_err(|_| SinkError::Timeout("sending metadata"))??;
        let status = timeout(CONNECT_TIMEOUT, read_status(&mut stream))
            .await
            .map_err(|_| SinkError::Timeout("waiting for the metadata answer"))??;
        match status {
            (200, _) => Ok(()),
            (_, line) => Err(SinkError::Refused(line)),
        }
    }

    fn disconnect(&mut self) {
        self.stream = None;
    }

    fn take_connection(&mut self) -> Option<HandedConnection> {
        self.stream.take()?.hand_over().ok()
    }
}

/// Reads an HTTP response header and returns the status code and line.
async fn read_status(stream: &mut (impl AsyncRead + Unpin)) -> Result<(u16, String), SinkError> {
    let mut header = Vec::new();
    let mut byte = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") && !header.ends_with(b"\n\n") {
        if header.len() >= MAX_RESPONSE_HEADER || stream.read(&mut byte).await? == 0 {
            break;
        }
        header.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&header);
    let line = text.lines().next().unwrap_or_default().trim().to_owned();
    let code = line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| {
            SinkError::Refused(if line.is_empty() {
                "the server closed the connection".to_owned()
            } else {
                format!("unexpected answer {line:?}")
            })
        })?;
    Ok((code, line))
}

/// Header values cannot hold line breaks.
fn header_value(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use tokio::net::TcpListener;

    use super::*;

    fn config(port: u16) -> IcecastConfig {
        IcecastConfig::parse(
            "test",
            &serde_json::json!({
                "host": "127.0.0.1",
                "port": port,
                "mount": "/live",
                "password": "hackme",
                "name": "Heron\r\nFM",
            }),
        )
        .unwrap()
    }

    fn info() -> StreamInfo {
        StreamInfo {
            content_type: "audio/mpeg",
            sample_rate: 44_100,
            channels: 2,
            bitrate_kbps: Some(128),
        }
    }

    #[test]
    fn rejects_invalid_settings() {
        for config in [
            serde_json::json!({"host": "h", "mount": "live", "password": "p"}),
            serde_json::json!({"host": "h", "mount": "/a b", "password": "p"}),
            serde_json::json!({"host": "", "mount": "/live", "password": "p"}),
            serde_json::json!({"host": "h", "mount": "/live"}),
            serde_json::json!({"host": "h", "mount": "/live", "password": "p", "typo": 1}),
            serde_json::json!({"host": "h", "mount": "/live", "password": "p", "public_url": "ftp://x"}),
            serde_json::json!({"host": "h", "mount": "/live", "password": "p", "public_url": "http://a b"}),
        ] {
            assert!(IcecastConfig::parse("test", &config).is_err(), "{config}");
        }
    }

    #[test]
    fn listen_url_prefers_the_public_url() {
        assert_eq!(config(8000).listen_url(), "http://127.0.0.1:8000/live");
        for public_url in ["https://stream.example.org", "https://stream.example.org/"] {
            let config = IcecastConfig::parse(
                "test",
                &serde_json::json!({
                    "host": "icecast",
                    "mount": "/live",
                    "password": "p",
                    "public_url": public_url,
                }),
            )
            .unwrap();
            assert_eq!(config.listen_url(), "https://stream.example.org/live");
        }
        let empty = IcecastConfig::parse(
            "test",
            &serde_json::json!({"host": "h", "mount": "/live", "password": "p", "public_url": ""}),
        )
        .unwrap();
        assert_eq!(empty.listen_url(), "http://h:8000/live");
    }

    #[test]
    fn source_request_announces_the_stream() {
        let request = IcecastSink::new(config(8000), info()).source_request();
        assert!(request.starts_with("PUT /live HTTP/1.1\r\n"));
        // "source:hackme"
        assert!(request.contains("Authorization: Basic c291cmNlOmhhY2ttZQ==\r\n"));
        assert!(request.contains("Content-Type: audio/mpeg\r\n"));
        assert!(
            request.contains(
                "Ice-Audio-Info: ice-samplerate=44100;ice-channels=2;ice-bitrate=128\r\n"
            )
        );
        assert!(request.contains("Ice-Name: Heron  FM\r\n"));
        assert!(request.ends_with("\r\n\r\n"));
    }

    /// A fake Icecast server answering `answer` to the first request.
    async fn server(answer: &'static str) -> (u16, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = Vec::new();
            let mut buffer = [0_u8; 4096];
            while !received.windows(4).any(|w| w == b"\r\n\r\n") {
                let read = socket.read(&mut buffer).await.unwrap();
                received.extend_from_slice(&buffer[..read]);
            }
            socket.write_all(answer.as_bytes()).await.unwrap();
            loop {
                match socket.read(&mut buffer).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => received.extend_from_slice(&buffer[..read]),
                }
            }
            received
        });
        (port, task)
    }

    #[tokio::test]
    async fn streams_after_the_server_accepts() {
        let (port, server) = server("HTTP/1.1 100 Continue\r\n\r\n").await;
        let mut sink = IcecastSink::new(config(port), info());
        sink.connect().await.unwrap();
        sink.send(b"MP3DATA").await.unwrap();
        sink.disconnect();
        let received = server.await.unwrap();
        assert!(received.ends_with(b"\r\n\r\nMP3DATA"));
    }

    #[tokio::test]
    async fn reports_wrong_credentials() {
        let (port, _server) = server("HTTP/1.0 401 Authentication Required\r\n\r\n").await;
        let mut sink = IcecastSink::new(config(port), info());
        let error = sink.connect().await.unwrap_err();
        assert!(error.to_string().contains("password"), "{error}");
    }

    #[tokio::test]
    async fn updates_metadata_through_the_admin_endpoint() {
        let (port, server) = server("HTTP/1.0 200 OK\r\n\r\n<return>1</return>").await;
        let mut sink = IcecastSink::new(config(port), info());
        sink.update_metadata(&NowPlaying {
            artist: Some("Sigur Rós".into()),
            title: Some("Hoppípolla & more".into()),
        })
        .await
        .unwrap();
        let received = String::from_utf8(server.await.unwrap()).unwrap();
        assert!(
            received.starts_with(
                "GET /admin/metadata?mount=%2Flive&mode=updinfo&charset=UTF-8&song=Sigur%20R%C3%B3s%20%2D%20Hopp%C3%ADpolla%20%26%20more HTTP/1.0\r\n"
            ),
            "{received}"
        );
    }
}
