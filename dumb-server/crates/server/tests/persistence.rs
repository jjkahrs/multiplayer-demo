//! Profile persistence against a real MySQL. Skips unless `TEST_DATABASE_URL`
//! is set, e.g. (PowerShell, from dumb-server/):
//!   $env:TEST_DATABASE_URL="mysql://demo:demo@127.0.0.1:3306/demo"; cargo test -p server --test persistence

mod common;

use common::*;
use protocol::ClientMsg;
use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;

async fn test_pool() -> Option<MySqlPool> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL not set: skipping persistence test");
        return None;
    };
    Some(MySqlPoolOptions::new().connect(&url).await.expect("connect to TEST_DATABASE_URL"))
}

/// `(pos_x, pos_z)` of the profile a "Bob" join would load.
async fn bob_saved_position(pool: &MySqlPool) -> (f64, f64) {
    sqlx::query_as(
        "SELECT pos_x, pos_z FROM profiles WHERE display_name = 'Bob' ORDER BY updated_at DESC, id DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn bob_rows(pool: &MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE display_name = 'Bob'")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
    let eps = 1e-9;
    assert!(
        (actual.0 - expected.0).abs() < eps && (actual.1 - expected.1).abs() < eps,
        "{actual:?} != {expected:?}"
    );
}

// One test so its scenarios never race each other on the shared "Bob" rows.
#[tokio::test]
async fn rejoin_restores_position_and_duplicate_names_share_a_profile() {
    let Some(pool) = test_pool().await else { return };
    let addr = start_server(Some(pool.clone())).await;
    let mut watcher = connect(&addr).await;
    join(&mut watcher, "Watcher").await;

    // 1. Move, disconnect, rejoin: the joined position is the saved one.
    let mut bob = connect(&addr).await;
    let (bob_id, _, z0) = join_at(&mut bob, "Bob").await;
    let vz = if z0 > 0.0 { -1.0 } else { 1.0 }; // toward the center, never into a wall
    send(&mut bob, ClientMsg::Input { vx: 0.0, vz, seq: 1, t0: 1 }).await;
    snapshot_until(&mut watcher, |p| find(p, bob_id).is_some_and(|b| (b.z - z0).abs() > 1.0)).await;
    bob.close(None).await.unwrap();
    // Gone from snapshots = grace expired and the save was queued.
    snapshot_until(&mut watcher, |p| find(p, bob_id).is_none()).await;

    let mut bob = connect(&addr).await;
    let (rejoined_id, x, z) = join_at(&mut bob, "Bob").await;
    assert_ne!(rejoined_id, bob_id, "a rejoin is a new session");
    assert!((z - z0).abs() > 1.0, "rejoined at z={z}, expected the moved position, not z0={z0}");
    assert_close((x, z), bob_saved_position(&pool).await);
    bob.close(None).await.unwrap();
    snapshot_until(&mut watcher, |p| find(p, rejoined_id).is_none()).await;

    // 2. Two concurrent "Bob"s: both accepted, distinct ids, one shared profile row.
    let rows_before = bob_rows(&pool).await;
    assert!(rows_before >= 1);
    let mut bob_a = connect(&addr).await;
    let mut bob_b = connect(&addr).await;
    let ((a_id, _, _), (b_id, _, _)) = tokio::join!(join_at(&mut bob_a, "Bob"), join_at(&mut bob_b, "Bob"));
    assert_ne!(a_id, b_id);
    snapshot_until(&mut watcher, |p| find(p, a_id).is_some() && find(p, b_id).is_some()).await;
    bob_a.close(None).await.unwrap();
    bob_b.close(None).await.unwrap();
    snapshot_until(&mut watcher, |p| find(p, a_id).is_none() && find(p, b_id).is_none()).await;
    assert_eq!(bob_rows(&pool).await, rows_before, "both Bobs reused the existing profile row");

    // A final join is queued behind both saves, so it proves they landed and
    // loads the last-written position.
    let mut bob = connect(&addr).await;
    let (_, x, z) = join_at(&mut bob, "Bob").await;
    assert_close((x, z), bob_saved_position(&pool).await);
}
