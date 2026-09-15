//! End-to-end network simulation: round-trip delay over real sockets, queued
//! replies delivered before close, and no delay when simulation is off.

mod common;

use std::time::{Duration, Instant};

use protocol::{ClientMsg, ServerMsg};
use server::netsim::NetSim;

const LAG: NetSim = NetSim { latency_ms: 200, jitter_ms: 0 };

#[tokio::test]
async fn joined_reply_delayed_by_round_trip() {
    let addr = common::start_server_with(None, LAG).await;
    let mut client = common::connect(&addr).await;

    let t = Instant::now();
    common::send(&mut client, ClientMsg::Join { name: "Lag".to_owned() }).await;
    let reply = common::recv(&mut client).await;
    let elapsed = t.elapsed();

    assert!(matches!(reply, Some(ServerMsg::Joined { .. })), "{reply:?}");
    assert!(elapsed >= Duration::from_millis(200), "{elapsed:?}");
    assert!(elapsed < common::TIMEOUT, "{elapsed:?}");
}

/// Uses `leave`, not a client Close frame: once the server reads a Close,
/// tungstenite refuses further data frames (`SendAfterClosing`).
#[tokio::test]
async fn reply_delivered_after_leave() {
    let addr = common::start_server_with(None, LAG).await;
    let mut client = common::connect(&addr).await;

    common::send_raw(&mut client, "not json").await;
    common::send(&mut client, ClientMsg::Leave).await;

    match common::recv(&mut client).await {
        Some(ServerMsg::ErrorMsg { code, .. }) => assert_eq!(code, "bad_message"),
        other => panic!("expected bad_message before close, got {other:?}"),
    }
    assert!(common::recv(&mut client).await.is_none());
}

#[tokio::test]
async fn no_delay_when_off() {
    let addr = common::start_server_with(None, NetSim::default()).await;
    let mut client = common::connect(&addr).await;

    let t = Instant::now();
    common::join(&mut client, "Fast").await;
    let elapsed = t.elapsed();

    assert!(elapsed < Duration::from_millis(100), "{elapsed:?}");
}
