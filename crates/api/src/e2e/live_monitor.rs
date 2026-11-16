// SPDX-License-Identifier: AGPL-3.0-or-later

//! The live monitor (M20): the station's sound over a WebSocket, 20 ms at a
//! time, before or after the processing, for operators and admins.

use futures_util::SinkExt;

use super::*;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Opens the monitor as `client`, or gives the HTTP status it was refused with.
async fn open(station: &Station, client: &Client, query: &str) -> Result<Socket, u16> {
    let url = format!("ws://{}/api/v1/monitor{query}", station.api);
    let mut request = tungstenite::client::IntoClientRequest::into_client_request(url).unwrap();
    request
        .headers_mut()
        .insert("cookie", client.cookie.as_deref().unwrap().parse().unwrap());
    match tokio_tungstenite::connect_async(request).await {
        Ok((socket, _)) => Ok(socket),
        Err(tungstenite::Error::Http(response)) => Err(response.status().as_u16()),
        Err(error) => panic!("monitor: {error}"),
    }
}

async fn next(socket: &mut Socket) -> tungstenite::Message {
    tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("a monitor message")
        .unwrap()
        .unwrap()
}

async fn text(socket: &mut Socket) -> Value {
    loop {
        if let tungstenite::Message::Text(text) = next(socket).await {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_monitor_streams_both_taps() {
    let Some(station) = Station::start().await else {
        return;
    };
    let admin = station.signed_in("admin").await;

    // PCM: 3,840 bytes per 20 ms, in real time, and sound once a track plays.
    let mut socket = open(&station, &admin, "?codec=pcm&tap=raw").await.unwrap();
    assert_eq!(
        text(&mut socket).await,
        json!({"type": "ready", "codec": "pcm", "sample_rate": 48000, "channels": 2,
               "frame_ms": 20, "tap": "raw"})
    );
    let started = Instant::now();
    let mut chunks = 0;
    let mut loud = false;
    while chunks < 100 {
        if let tungstenite::Message::Binary(audio) = next(&mut socket).await {
            assert_eq!(audio.len(), 3_840);
            loud |= audio
                .chunks_exact(2)
                .any(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]).unsigned_abs() > 1_000);
            chunks += 1;
        }
    }
    // Two seconds of sound, as it plays: not much faster, not much slower.
    let elapsed = started.elapsed();
    assert!(
        elapsed > Duration::from_millis(1_500) && elapsed < Duration::from_secs(4),
        "{elapsed:?}"
    );
    assert!(loud, "the monitor is silent");

    // Switching taps is told, and the sound goes on.
    socket
        .send(tungstenite::Message::Text(r#"{"tap": "processed"}"#.into()))
        .await
        .unwrap();
    assert_eq!(
        text(&mut socket).await,
        json!({"type": "tap", "tap": "processed"})
    );
    assert!(matches!(
        next(&mut socket).await,
        tungstenite::Message::Binary(_)
    ));

    // Opus: one packet per 20 ms, about 640 bytes at 256 kbit/s.
    let mut opus = open(&station, &admin, "").await.unwrap();
    assert_eq!(
        text(&mut opus).await,
        json!({"type": "ready", "codec": "opus", "sample_rate": 48000, "channels": 2,
               "frame_ms": 20, "bitrate": 256000, "tap": "processed"})
    );
    let mut bytes = 0;
    for _ in 0..50 {
        match next(&mut opus).await {
            tungstenite::Message::Binary(packet) => {
                assert!(!packet.is_empty() && packet.len() < 4_000);
                bytes += packet.len();
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(bytes / 50 < 1_500, "{} bytes", bytes / 50);

    // Viewers may not listen.
    let viewer = station.signed_in("viewer").await;
    assert_eq!(open(&station, &viewer, "").await.err(), Some(403));

    // Eight listeners per station: two already, six more, not a ninth.
    let mut others = Vec::new();
    for _ in 0..6 {
        others.push(open(&station, &admin, "?codec=pcm").await.unwrap());
    }
    assert_eq!(open(&station, &admin, "").await.err(), Some(429));
    // One leaves: room for another.
    drop(others.pop());
    let back = eventually("room for a listener", Duration::from_secs(5), || async {
        open(&station, &admin, "").await.ok()
    })
    .await;
    drop(back);

    // The first listener still hears the station.
    assert!(matches!(
        next(&mut socket).await,
        tungstenite::Message::Binary(_)
    ));
}
