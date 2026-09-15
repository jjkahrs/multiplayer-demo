//! End-to-end: real WebSocket clients against an in-process server and Zone
//! (no database).

mod common;

use std::time::{Duration, Instant};

use common::*;
use protocol::{ClientMsg, PlayerState, ServerMsg};

#[tokio::test]
async fn join_move_peer_and_disconnect() {
    let addr = start_server(None).await;

    let mut alice = connect(&addr).await;
    let alice_id = join(&mut alice, "Alice").await;
    players_metric_is(&addr, 1).await;

    // 20 Hz: 10 snapshots after a reference one span 10 ticks, ~500 ms. The bound
    // is tight enough to catch a drifting ticker (16 Hz on Windows timers = ~620 ms).
    next_snapshot(&mut alice).await;
    let started = Instant::now();
    for _ in 0..10 {
        let players = next_snapshot(&mut alice).await;
        assert!(find(&players, alice_id).is_some());
    }
    let elapsed = started.elapsed();
    assert!(
        (Duration::from_millis(450)..=Duration::from_millis(600)).contains(&elapsed),
        "10 snapshots took {elapsed:?}"
    );

    send(&mut alice, ClientMsg::Input { vx: 1.0, vz: 0.0, seq: 1, t0: 123 }).await;
    let players = snapshot_until(&mut alice, |p| find(p, alice_id).is_some_and(|a| a.x > 0.5)).await;
    let first = find(&players, alice_id).unwrap().clone();
    assert_eq!((first.state, first.seq, first.t0), (PlayerState::Walk, 1, 123));
    let players = snapshot_until(&mut alice, |p| find(p, alice_id).is_some_and(|a| a.x > first.x)).await;
    assert!(find(&players, alice_id).unwrap().x > first.x, "x keeps advancing");

    let mut bob = connect(&addr).await;
    let bob_id = join(&mut bob, "Bob").await;
    assert!(bob_id > alice_id, "ids are monotonic");
    let announced = recv_until(&mut alice, |m| matches!(m, ServerMsg::PlayerJoined { .. })).await;
    assert_eq!(announced, ServerMsg::PlayerJoined { id: bob_id, name: "Bob".to_owned() });
    let players = snapshot_until(&mut bob, |p| find(p, alice_id).is_some() && find(p, bob_id).is_some()).await;
    assert_eq!(find(&players, alice_id).unwrap().name, "Alice");
    players_metric_is(&addr, 2).await;

    // Socket close: walking Alice stays in snapshots, frozen and idle, for the
    // grace period; then Bob is told she left and she is gone.
    alice.close(None).await.unwrap();
    let closed_at = Instant::now();
    let mut frozen = None;
    loop {
        match recv(&mut bob).await.expect("bob disconnected") {
            ServerMsg::Snapshot { players } => {
                let a = find(&players, alice_id).expect("Alice removed before playerLeft");
                if a.state == PlayerState::Idle {
                    let (x, z) = *frozen.get_or_insert((a.x, a.z));
                    assert_eq!((a.x, a.z), (x, z), "suspended Alice must not move");
                }
            }
            ServerMsg::PlayerLeft { id } => {
                assert_eq!(id, alice_id);
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(closed_at.elapsed() >= GRACE, "left after {:?}, before the grace period", closed_at.elapsed());
    assert!(frozen.is_some(), "Alice was never shown idle during grace");
    assert!(find(&next_snapshot(&mut bob).await, alice_id).is_none());
    players_metric_is(&addr, 1).await;

    // A rejoin is a new session with a fresh, higher id.
    let mut alice = connect(&addr).await;
    let rejoined_id = join(&mut alice, "Alice").await;
    assert!(rejoined_id > bob_id, "ids are never reused");
    alice.close(None).await.unwrap();

    // Explicit leave: removed after grace, and the server closes Bob's socket.
    send(&mut bob, ClientMsg::Leave).await;
    players_metric_is(&addr, 0).await;
    while recv(&mut bob).await.is_some() {}
}

#[tokio::test]
async fn bad_names_are_rejected_without_closing_and_names_are_trimmed() {
    let addr = start_server(None).await;
    let mut client = connect(&addr).await;

    for bad in ["", "   ", "12345678901234567", "Bo\u{1}b"] {
        send(&mut client, ClientMsg::Join { name: bad.to_owned() }).await;
        expect_error(&mut client, "bad_name").await;
    }
    players_metric_is(&addr, 0).await;

    // The same socket can still join; surrounding whitespace is trimmed.
    send(&mut client, ClientMsg::Join { name: "  a  ".to_owned() }).await;
    match recv(&mut client).await {
        Some(ServerMsg::Joined { name, .. }) => assert_eq!(name, "a"),
        other => panic!("expected joined, got {other:?}"),
    }
    let players = snapshot_until(&mut client, |p| p.len() == 1).await;
    assert_eq!(players[0].name, "a");
}

#[tokio::test]
async fn junk_frames_get_bad_message_and_the_socket_stays_open() {
    let addr = start_server(None).await;
    let mut client = connect(&addr).await;

    for junk in ["not json", r#"{"type":"fly"}"#, r#"{"type":"input","vx":"fast"}"#, "{}"] {
        send_raw(&mut client, junk).await;
        expect_error(&mut client, "bad_message").await;
    }

    // Mid-session junk doesn't disturb the session either.
    let id = join(&mut client, "Alice").await;
    send_raw(&mut client, "}{").await;
    expect_error(&mut client, "bad_message").await;
    send(&mut client, ClientMsg::Input { vx: 1.0, vz: 0.0, seq: 1, t0: 1 }).await;
    snapshot_until(&mut client, |p| find(p, id).is_some_and(|a| a.x > 0.0)).await;
}

#[tokio::test]
async fn out_of_range_and_burst_input_are_tolerated() {
    let addr = start_server(None).await;
    let mut client = connect(&addr).await;
    let (id, x0, z0) = join_at(&mut client, "Alice").await;

    send(&mut client, ClientMsg::Input { vx: 1.5, vz: 0.0, seq: 1, t0: 1 }).await;
    for _ in 0..5 {
        next_snapshot(&mut client).await;
    }
    let players = next_snapshot(&mut client).await;
    let alice = find(&players, id).unwrap();
    assert_eq!((alice.x, alice.z, alice.seq), (x0, z0, 0), "vx=1.5 input must be ignored");

    for seq in 1..=500 {
        send(&mut client, ClientMsg::Input { vx: 1.0, vz: 0.0, seq, t0: seq }).await;
    }
    snapshot_until(&mut client, |p| find(p, id).is_some_and(|a| a.x > x0 + 0.5)).await;
    players_metric_is(&addr, 1).await;
}

#[tokio::test]
async fn duplicate_names_are_both_accepted_with_distinct_ids() {
    let addr = start_server(None).await;
    let mut bob_a = connect(&addr).await;
    let mut bob_b = connect(&addr).await;
    let a = join(&mut bob_a, "Bob").await;
    let b = join(&mut bob_b, "Bob").await;
    assert_ne!(a, b);

    let players = snapshot_until(&mut bob_a, |p| find(p, a).is_some() && find(p, b).is_some()).await;
    assert!(players.iter().all(|p| p.name == "Bob"));
}

#[tokio::test]
async fn input_before_join_is_ignored_and_close_before_joined_leaves_no_ghost() {
    let addr = start_server(None).await;

    let mut early = connect(&addr).await;
    send(&mut early, ClientMsg::Input { vx: 1.0, vz: 0.0, seq: 1, t0: 1 }).await;
    // Join then close immediately, racing the `joined` reply.
    send(&mut early, ClientMsg::Join { name: "Ghost".to_owned() }).await;
    early.close(None).await.unwrap();

    let mut watcher = connect(&addr).await;
    let watcher_id = join(&mut watcher, "Watcher").await;
    let players = snapshot_until(&mut watcher, |p| p.len() == 1).await;
    assert_eq!(players[0].id, watcher_id);
    players_metric_is(&addr, 1).await;
}
