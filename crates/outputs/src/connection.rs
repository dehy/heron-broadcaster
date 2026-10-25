// SPDX-License-Identifier: AGPL-3.0-or-later

//! The connection of a network output: TCP to the stream server, or a Unix
//! socket to `heron-engine relay`, which holds the TCP connection for it.
//!
//! Through the relay, the TCP connection lives in the relay's container:
//! an engine can hand its Unix connection to another engine, in another
//! container, and the stream server never sees the source leave
//! (`docs/handover.md`). A TCP socket handed to another container would die
//! with the first one's network.

use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf,
};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::time::timeout;

use crate::sink::SinkError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// `CONNECT host:port` lines longer than this are refused.
const MAX_REQUEST: usize = 512;

/// An open connection to a stream server.
pub enum Connection {
    Tcp(TcpStream),
    /// Through the relay.
    Relayed(UnixStream),
}

/// What kind of connection is handed over, to rebuild it on the other side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    Tcp,
    Relayed,
}

/// A connection taken out of the runtime, to hand over to another engine.
pub enum HandedConnection {
    Tcp(std::net::TcpStream),
    Relayed(std::os::unix::net::UnixStream),
}

impl HandedConnection {
    pub fn kind(&self) -> ConnectionKind {
        match self {
            Self::Tcp(_) => ConnectionKind::Tcp,
            Self::Relayed(_) => ConnectionKind::Relayed,
        }
    }

    /// Rebuilds a connection received as a file descriptor.
    pub fn from_fd(kind: ConnectionKind, fd: std::os::fd::OwnedFd) -> Self {
        match kind {
            ConnectionKind::Tcp => Self::Tcp(fd.into()),
            ConnectionKind::Relayed => Self::Relayed(fd.into()),
        }
    }
}

impl From<HandedConnection> for std::os::fd::OwnedFd {
    fn from(connection: HandedConnection) -> Self {
        match connection {
            HandedConnection::Tcp(stream) => stream.into(),
            HandedConnection::Relayed(stream) => stream.into(),
        }
    }
}

impl Connection {
    /// Connects to `host:port`, through the relay listening on `relay` when
    /// given. A relay that cannot be reached is skipped: better on air with
    /// a direct connection than off air.
    pub async fn open(host: &str, port: u16, relay: Option<&Path>) -> Result<Self, SinkError> {
        if let Some(relay) = relay {
            match timeout(CONNECT_TIMEOUT, UnixStream::connect(relay)).await {
                Ok(Ok(stream)) => return relayed(stream, host, port).await,
                Ok(Err(error)) => {
                    tracing::warn!(relay = %relay.display(), %error, "relay unreachable: connecting directly");
                }
                Err(_) => {
                    tracing::warn!(relay = %relay.display(), "relay too slow: connecting directly")
                }
            }
        }
        let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
            .await
            .map_err(|_| SinkError::Timeout("connecting"))?
            .map_err(SinkError::Connect)?;
        stream.set_nodelay(true)?;
        Ok(Self::Tcp(stream))
    }

    /// Takes a handed-over connection into the runtime.
    pub fn adopt(connection: HandedConnection) -> io::Result<Self> {
        Ok(match connection {
            HandedConnection::Tcp(stream) => {
                stream.set_nonblocking(true)?;
                Self::Tcp(TcpStream::from_std(stream)?)
            }
            HandedConnection::Relayed(stream) => {
                stream.set_nonblocking(true)?;
                Self::Relayed(UnixStream::from_std(stream)?)
            }
        })
    }

    /// Takes the connection out of the runtime, still open.
    pub fn hand_over(self) -> io::Result<HandedConnection> {
        Ok(match self {
            Self::Tcp(stream) => HandedConnection::Tcp(stream.into_std()?),
            Self::Relayed(stream) => HandedConnection::Relayed(stream.into_std()?),
        })
    }
}

/// Asks the relay for `host:port`.
async fn relayed(mut stream: UnixStream, host: &str, port: u16) -> Result<Connection, SinkError> {
    if host.contains(char::is_whitespace) {
        return Err(SinkError::Refused(format!("invalid host {host:?}")));
    }
    stream
        .write_all(format!("CONNECT {host}:{port}\n").as_bytes())
        .await?;
    // Byte by byte: nothing after the answer may be read here.
    let mut answer = Vec::new();
    let mut byte = [0_u8; 1];
    let read = async {
        while !answer.ends_with(b"\n") && answer.len() < MAX_REQUEST {
            if tokio::io::AsyncReadExt::read(&mut stream, &mut byte).await? == 0 {
                break;
            }
            answer.push(byte[0]);
        }
        io::Result::Ok(())
    };
    timeout(CONNECT_TIMEOUT + Duration::from_secs(1), read)
        .await
        .map_err(|_| SinkError::Timeout("waiting for the relay"))??;
    let answer = String::from_utf8_lossy(&answer);
    let answer = answer.trim_end();
    if answer == "OK" {
        return Ok(Connection::Relayed(stream));
    }
    let reason = answer.strip_prefix("ERR ").unwrap_or(answer);
    Err(SinkError::Connect(io::Error::other(format!(
        "through the relay: {reason}"
    ))))
}

impl AsyncRead for Connection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Relayed(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Relayed(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_flush(cx),
            Self::Relayed(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Relayed(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

/// The relay (`heron-engine relay`): listens on `path`, and for each client
/// connects to the `host:port` it asks for (`CONNECT host:port\n`, answered
/// `OK\n` or `ERR <reason>\n`), then carries bytes both ways until either
/// side closes. Runs until the task is dropped.
pub async fn serve_relay(path: PathBuf) -> io::Result<()> {
    match std::fs::remove_file(&path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let listener = UnixListener::bind(&path)?;
    tracing::info!(path = %path.display(), "relay listening");
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(error) = relay(stream).await {
                tracing::warn!(%error, "relayed connection ended");
            }
        });
    }
}

async fn relay(stream: UnixStream) -> io::Result<()> {
    let mut client = BufReader::new(stream);
    let mut request = String::new();
    timeout(
        CONNECT_TIMEOUT,
        (&mut client)
            .take(MAX_REQUEST as u64)
            .read_line(&mut request),
    )
    .await
    .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
    if request.is_empty() {
        // A health check: connected, then closed.
        return Ok(());
    }
    let Some(target) = request
        .trim_end()
        .strip_prefix("CONNECT ")
        .and_then(|target| target.rsplit_once(':'))
        .and_then(|(host, port)| Some((host.to_owned(), port.parse::<u16>().ok()?)))
    else {
        client.get_mut().write_all(b"ERR invalid request\n").await?;
        return Ok(());
    };
    let server = match timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((target.0.as_str(), target.1)),
    )
    .await
    {
        Ok(Ok(server)) => server,
        Ok(Err(error)) => {
            let answer = format!("ERR {error}\n");
            client.get_mut().write_all(answer.as_bytes()).await?;
            return Ok(());
        }
        Err(_) => {
            client.get_mut().write_all(b"ERR timed out\n").await?;
            return Ok(());
        }
    };
    server.set_nodelay(true)?;
    client.get_mut().write_all(b"OK\n").await?;
    tracing::info!(host = %target.0, port = target.1, "relaying");
    // Anything the client sent after its request is in the reader's buffer.
    let mut server = server;
    let buffered = client.buffer().to_vec();
    if !buffered.is_empty() {
        server.write_all(&buffered).await?;
    }
    let mut client = client.into_inner();
    let (sent, received) = tokio::io::copy_bidirectional(&mut client, &mut server).await?;
    tracing::info!(host = %target.0, port = target.1, sent, received, "relayed connection closed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    async fn relays_both_ways_and_survives_a_handover() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.sock");
        tokio::spawn(serve_relay(path.clone()));
        let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        while !path.exists() {
            tokio::task::yield_now().await;
        }

        let mut connection = Connection::open("127.0.0.1", port, Some(&path))
            .await
            .unwrap();
        assert!(matches!(connection, Connection::Relayed(_)));
        let (mut accepted, _) = server.accept().await.unwrap();
        connection.write_all(b"hello").await.unwrap();
        let mut buffer = [0_u8; 5];
        accepted.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"hello");

        // Handed over (as another engine would get it), then adopted.
        let handed = connection.hand_over().unwrap();
        assert_eq!(handed.kind(), ConnectionKind::Relayed);
        let fd = std::os::fd::OwnedFd::from(handed);
        let mut adopted =
            Connection::adopt(HandedConnection::from_fd(ConnectionKind::Relayed, fd)).unwrap();
        adopted.write_all(b"again").await.unwrap();
        accepted.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"again");
        accepted.write_all(b"pong!").await.unwrap();
        adopted.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"pong!");

        // Closed on the engine's side: the relay closes the server's side.
        drop(adopted);
        assert_eq!(accepted.read(&mut buffer).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn connects_directly_without_the_relay() {
        let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        let missing = Path::new("/nonexistent/relay.sock");
        let connection = Connection::open("127.0.0.1", port, Some(missing))
            .await
            .unwrap();
        assert!(matches!(connection, Connection::Tcp(_)));
    }

    #[tokio::test]
    async fn reports_what_the_relay_cannot_reach() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.sock");
        tokio::spawn(serve_relay(path.clone()));
        while !path.exists() {
            tokio::task::yield_now().await;
        }
        // A port nothing listens on.
        let free = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        let error = Connection::open("127.0.0.1", port, Some(&path))
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("through the relay"), "{error}");
    }
}
