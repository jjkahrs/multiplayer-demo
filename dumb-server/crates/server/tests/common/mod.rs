//! Shared helpers for server integration tests: an in-process server and a
//! small WebSocket client toolkit.
#![allow(dead_code)] // Each test crate uses a different subset.

use std::future::Future;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMsg, ServerMsg, SnapshotPlayer};
use server::config::Config;
use server::netsim::NetSim;
use sqlx::MySqlPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

pub type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub const TIMEOUT: Duration = Duration::from_secs(2);
/// Short disconnect grace so removal-timeline tests stay fast.
pub const GRACE: Duration = Duration::from_millis(200);

/// Start a server + Zone on an ephemeral port; returns `host:port`.
pub async fn start_server(pool: Option<MySqlPool>) -> String {
    start_server_with(pool, NetSim::default()).await
}

/// [`start_server`] with simulated network conditions.
pub async fn start_server_with(pool: Option<MySqlPool>, net_sim: NetSim) -> String {
    let config = Config {
        bind: String::new(),
        tick_hz: 20,
        grace_ms: GRACE.as_millis() as u64,
        speed: 5.0,
        world_half: 50.0,
        database_url: None,
        net_sim,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let app = server::http::router(server::zone::spawn(&config, pool), net_sim);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

pub async fn connect(addr: &str) -> Client {
    connect_async(format!("ws://{addr}/ws")).await.unwrap().0
}

pub async fn send(client: &mut Client, msg: ClientMsg) {
    let text = serde_json::to_string(&msg).unwrap();
    client.send(Message::Text(text.into())).await.unwrap();
}

/// Send an arbitrary text frame (for junk-input tests).
pub async fn send_raw(client: &mut Client, text: &str) {
    client.send(Message::Text(text.to_owned().into())).await.unwrap();
}

/// Wait for the next `error` reply, skipping other frames, and check its code.
pub async fn expect_error(client: &mut Client, code: &str) {
    match recv_until(client, |m| matches!(m, ServerMsg::ErrorMsg { .. })).await {
        ServerMsg::ErrorMsg { code: actual, .. } => assert_eq!(actual, code),
        _ => unreachable!(),
    }
}

/// Next text frame as a server message; `None` once the server closes.
pub async fn recv(client: &mut Client) -> Option<ServerMsg> {
    loop {
        match timeout(TIMEOUT, client.next()).await.expect("timed out waiting for a frame") {
            Some(Ok(Message::Text(text))) => return Some(serde_json::from_str(&text).unwrap()),
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return None,
            Some(Ok(_)) => continue,
        }
    }
}

/// Join and return `(player_id, x, z)` from the `joined` reply.
pub async fn join_at(client: &mut Client, name: &str) -> (u64, f64, f64) {
    send(client, ClientMsg::Join { name: name.to_owned() }).await;
    match recv(client).await {
        Some(ServerMsg::Joined { player_id, name: joined_name, x, z, .. }) => {
            assert_eq!(joined_name, name);
            (player_id, x, z)
        }
        other => panic!("expected joined as the first frame, got {other:?}"),
    }
}

pub async fn join(client: &mut Client, name: &str) -> u64 {
    join_at(client, name).await.0
}

/// Receive until `done` accepts a message, skipping everything else.
pub async fn recv_until(client: &mut Client, done: impl Fn(&ServerMsg) -> bool) -> ServerMsg {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let msg = recv(client).await.expect("connection closed while waiting for a message");
        if done(&msg) {
            return msg;
        }
        assert!(Instant::now() < deadline, "message condition not met within {TIMEOUT:?}");
    }
}

pub async fn next_snapshot(client: &mut Client) -> Vec<SnapshotPlayer> {
    loop {
        match recv(client).await {
            Some(ServerMsg::Snapshot { players }) => return players,
            Some(_) => continue,
            None => panic!("connection closed while waiting for a snapshot"),
        }
    }
}

pub async fn snapshot_until(client: &mut Client, done: impl Fn(&[SnapshotPlayer]) -> bool) -> Vec<SnapshotPlayer> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let players = next_snapshot(client).await;
        if done(&players) {
            return players;
        }
        assert!(Instant::now() < deadline, "snapshot condition not met within {TIMEOUT:?}");
    }
}

pub fn find(players: &[SnapshotPlayer], id: u64) -> Option<&SnapshotPlayer> {
    players.iter().find(|p| p.id == id)
}

/// Raw HTTP GET of `/metrics` (no HTTP client dependency needed).
pub async fn metrics(addr: &str) -> serde_json::Value {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET /metrics HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body = response.split("\r\n\r\n").nth(1).unwrap();
    serde_json::from_str(body).unwrap()
}

pub async fn eventually<F: Future<Output = bool>>(what: &str, check: impl Fn() -> F) {
    let deadline = Instant::now() + TIMEOUT;
    while !check().await {
        assert!(Instant::now() < deadline, "{what} not reached within {TIMEOUT:?}");
        sleep(Duration::from_millis(25)).await;
    }
}

pub async fn players_metric_is(addr: &str, expected: u64) {
    eventually(&format!("/metrics players == {expected}"), || async {
        metrics(addr).await["players"] == expected
    })
    .await;
}
