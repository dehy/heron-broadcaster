// SPDX-License-Identifier: AGPL-3.0-or-later

//! A tiny HTTP client for container health checks, so images do not need
//! `curl` or `wget`.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::Duration;

use crate::Error;

/// Sends `GET path` to the service listening on `listen_addr` and succeeds on a
/// `200` answer. An unspecified listen address (`0.0.0.0`, `::`) is probed on loopback.
pub fn probe(listen_addr: SocketAddr, path: &str, timeout: Duration) -> Result<(), Error> {
    let target = loopback_if_unspecified(listen_addr);
    let mut stream = TcpStream::connect_timeout(&target, timeout)
        .map_err(|error| Error::Probe(format!("connect to {target}: {error}")))?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .map_err(|error| Error::Probe(error.to_string()))?;

    let request = format!("GET {path} HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| Error::Probe(format!("send request: {error}")))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| Error::Probe(format!("read response: {error}")))?;

    let response = String::from_utf8_lossy(&response);
    let status_line = response.lines().next().unwrap_or_default();
    match status_line.split_whitespace().nth(1) {
        Some("200") => Ok(()),
        _ => Err(Error::Probe(format!("unexpected answer: {status_line:?}"))),
    }
}

fn loopback_if_unspecified(addr: SocketAddr) -> SocketAddr {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), addr.port())
        }
        _ => addr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn serve_once(answer: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).unwrap();
            stream.write_all(answer.as_bytes()).unwrap();
        });
        addr
    }

    #[test]
    fn accepts_200() {
        let addr = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
        probe(addr, "/healthz", Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn rejects_503() {
        let addr = serve_once("HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\n\r\n");
        assert!(probe(addr, "/healthz", Duration::from_secs(2)).is_err());
    }

    #[test]
    fn probes_loopback_for_unspecified_addresses() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        assert_eq!(
            loopback_if_unspecified(addr),
            "127.0.0.1:8080".parse().unwrap()
        );
    }
}
