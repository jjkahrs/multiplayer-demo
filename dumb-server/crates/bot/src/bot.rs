//! One bot: connect → join → random-walk inputs at 10 Hz → sample latency
//! from peers' echoed `t0` until the deadline.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMsg, ServerMsg};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::time::{Instant, interval, sleep_until, timeout};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const INPUT_PERIOD: Duration = Duration::from_millis(100);
const JOIN_TIMEOUT: Duration = Duration::from_secs(10);

/// What one bot observed over its run.
#[derive(Debug, Default)]
pub struct BotStats {
    /// Received `joined`.
    pub connected: bool,
    /// Connection ended before the deadline (server closed or errored).
    pub dropped: bool,
    pub snapshots: u64,
    /// Seconds between `joined` and the end of the run (or the drop).
    pub active_secs: f64,
    /// One-way latency samples in ms.
    pub latencies_ms: Vec<u64>,
}

/// Wall-clock ms. Shared by every process on this machine, so samples are
/// valid against any client that stamps `t0` the same way (see design "t0 echo").
fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

pub async fn run(index: usize, url: String, deadline: Instant) -> BotStats {
    let mut stats = BotStats::default();
    let Ok(Ok((mut ws, _))) = timeout(JOIN_TIMEOUT, connect_async(url.as_str())).await else {
        return stats;
    };
    let join = ClientMsg::Join { name: format!("Bot-{index}") };
    if ws.send(Message::Text(serde_json::to_string(&join).unwrap().into())).await.is_err() {
        return stats;
    }

    let Ok(Some(own_id)) = timeout(JOIN_TIMEOUT, async {
        while let Some(Ok(frame)) = ws.next().await {
            match parse(frame) {
                Some(ServerMsg::Joined { player_id, .. }) => return Some(player_id),
                Some(ServerMsg::ErrorMsg { .. }) => return None,
                _ => {}
            }
        }
        None
    })
    .await
    else {
        return stats;
    };
    stats.connected = true;
    let started = Instant::now();

    let mut rng = StdRng::seed_from_u64(index as u64);
    let mut direction = (0.0, 0.0);
    let mut reroll_at = Instant::now();
    let mut seq = 0u64;
    // Newest seq seen per peer: sample only when it advances, so each input is
    // measured once, at first arrival, instead of re-sampling a stale t0.
    let mut peer_seq: HashMap<u64, u64> = HashMap::new();
    let mut ticker = interval(INPUT_PERIOD);

    loop {
        tokio::select! {
            _ = sleep_until(deadline) => break,
            _ = ticker.tick() => {
                if Instant::now() >= reroll_at {
                    let angle = rng.random_range(0.0..std::f64::consts::TAU);
                    direction = (angle.cos(), angle.sin());
                    reroll_at = Instant::now() + Duration::from_millis(rng.random_range(1000..=3000));
                }
                seq += 1;
                let input = ClientMsg::Input { vx: direction.0, vz: direction.1, seq, t0: now_ms() };
                if ws.send(Message::Text(serde_json::to_string(&input).unwrap().into())).await.is_err() {
                    stats.dropped = true;
                    break;
                }
            }
            frame = ws.next() => {
                let Some(Ok(frame)) = frame else {
                    stats.dropped = true;
                    break;
                };
                if let Some(ServerMsg::Snapshot { players, .. }) = parse(frame) {
                    stats.snapshots += 1;
                    let now = now_ms();
                    for p in players.iter().filter(|p| p.id != own_id) {
                        let previous = peer_seq.insert(p.id, p.seq);
                        if previous.is_some_and(|s| p.seq > s) {
                            stats.latencies_ms.push(now.saturating_sub(p.t0));
                        }
                    }
                }
            }
        }
    }

    stats.active_secs = started.elapsed().as_secs_f64();
    if !stats.dropped {
        let _ = ws.send(Message::Text(serde_json::to_string(&ClientMsg::Leave).unwrap().into())).await;
        let _ = ws.close(None).await;
    }
    stats
}

/// Server frames may arrive as text or binary JSON; anything else is ignored.
fn parse(frame: Message) -> Option<ServerMsg> {
    match frame {
        Message::Text(text) => serde_json::from_str(&text).ok(),
        Message::Binary(bytes) => serde_json::from_slice(&bytes).ok(),
        _ => None,
    }
}
