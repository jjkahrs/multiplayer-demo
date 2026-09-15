//! Zone task: the single authority over world state. Connections talk to it
//! only through [`ClientEvent`]s; it answers through per-connection unicast
//! channels and one broadcast of pre-serialized snapshots per tick. Profile
//! I/O runs on a separate store task, so the database never delays a tick.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use protocol::{ServerMsg, SnapshotPlayer};
use sqlx::MySqlPool;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, Instant, MissedTickBehavior};

use crate::config::Config;
use crate::metrics::Metrics;
use crate::player::{Player, PlayerStatus};
use crate::profile::{self, Profile};

/// Pending client events before senders wait (150 players x 10 Hz input fits easily).
const EVENT_QUEUE: usize = 4096;
/// Broadcast backlog per subscriber; slow writers skip to the newest frame.
const OUTBOUND_BUFFER: usize = 4;
/// Minimum spacing between accepted inputs per player (caps input at 100/s).
const MIN_INPUT_GAP: Duration = Duration::from_millis(10);

/// Everything a connection can tell the Zone.
#[derive(Debug)]
pub enum ClientEvent {
    /// Enter the world; the `joined` reply goes to `unicast_tx`.
    Join { name: String, unicast_tx: mpsc::Sender<ServerMsg> },
    Input { player_id: u64, vx: f64, vz: f64, seq: u64, t0: u64 },
    Closed { player_id: u64 },
}

/// Frames broadcast to every connection.
#[derive(Debug, Clone)]
pub enum Outbound {
    /// A `snapshot` message, serialized once per tick for all receivers.
    Snapshot(Arc<[u8]>),
    Event(ServerMsg),
}

/// Handles for talking to a running Zone.
#[derive(Debug, Clone)]
pub struct ZoneHandle {
    pub events: mpsc::Sender<ClientEvent>,
    /// Call `subscribe()` per connection to receive [`Outbound`] frames.
    pub outbound: broadcast::Sender<Outbound>,
    pub metrics: Metrics,
}

/// Profile work for the store task, applied strictly in order: a save always
/// lands before a later load of the same name, and concurrent first joins of
/// one name can't both insert a row.
enum StoreRequest {
    Load { name: String, unicast_tx: mpsc::Sender<ServerMsg> },
    Save { profile_id: u64, x: f64, z: f64, yaw: f64 },
}

/// A join whose profile lookup finished (`profile` is `None` if it failed).
struct Loaded {
    name: String,
    unicast_tx: mpsc::Sender<ServerMsg>,
    profile: Option<Profile>,
}

/// Spawn the Zone task (and, with a pool, its profile store task). The Zone
/// runs until every `events` sender is dropped.
pub fn spawn(config: &Config, pool: Option<MySqlPool>) -> ZoneHandle {
    let (events, events_rx) = mpsc::channel(EVENT_QUEUE);
    let (outbound, _) = broadcast::channel(OUTBOUND_BUFFER);
    let (loaded_tx, loaded_rx) = mpsc::unbounded_channel();
    let store = pool.map(|pool| {
        let (store_tx, store_rx) = mpsc::unbounded_channel();
        tokio::spawn(run_store(pool, store_rx, loaded_tx));
        store_tx
    });
    let metrics = Metrics::default();
    let zone = Zone {
        speed: config.speed,
        world_half: config.world_half,
        grace: Duration::from_millis(config.grace_ms),
        next_player_id: 1,
        players: HashMap::new(),
        outbound: outbound.clone(),
        metrics: metrics.clone(),
        store,
    };
    tokio::spawn(zone.run(events_rx, loaded_rx, config.tick_hz.max(1)));
    ZoneHandle { events, outbound, metrics }
}

async fn run_store(
    pool: MySqlPool,
    mut requests: mpsc::UnboundedReceiver<StoreRequest>,
    loaded: mpsc::UnboundedSender<Loaded>,
) {
    while let Some(request) = requests.recv().await {
        match request {
            StoreRequest::Load { name, unicast_tx } => {
                let profile = match profile::load_or_create(&pool, &name).await {
                    Ok(profile) => Some(profile),
                    Err(err) => {
                        tracing::warn!(%err, name = %name, "profile load failed; joining without persistence");
                        None
                    }
                };
                if loaded.send(Loaded { name, unicast_tx, profile }).is_err() {
                    break; // Zone stopped.
                }
            }
            StoreRequest::Save { profile_id, x, z, yaw } => {
                if let Err(err) = profile::save(&pool, profile_id, x, z, yaw).await {
                    tracing::error!(%err, profile_id, "profile save failed");
                }
            }
        }
    }
}

struct Zone {
    speed: f64,
    world_half: f64,
    /// How long a disconnected player stays frozen in the world before removal.
    grace: Duration,
    /// Session ids are monotonic and never reused.
    next_player_id: u64,
    players: HashMap<u64, Player>,
    outbound: broadcast::Sender<Outbound>,
    metrics: Metrics,
    /// `None` when running without persistence.
    store: Option<mpsc::UnboundedSender<StoreRequest>>,
}

impl Zone {
    async fn run(
        mut self,
        mut events: mpsc::Receiver<ClientEvent>,
        mut loaded: mpsc::UnboundedReceiver<Loaded>,
        tick_hz: u64,
    ) {
        let mut ticker = interval(Duration::from_secs_f64(1.0 / tick_hz as f64));
        // Skip keeps ticks on the fixed grid. Delay re-bases on every late wake-up,
        // so Windows timer overshoot (~12 ms) accumulated into ~16 Hz instead of 20.
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut last_tick = Instant::now();
        tracing::info!(tick_hz, "zone started");

        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => self.handle(event),
                    None => break,
                },
                // Without a store every sender is gone and this branch stays disabled.
                Some(done) = loaded.recv() => self.admit(done.name, done.unicast_tx, done.profile),
                _ = ticker.tick() => {
                    let now = Instant::now();
                    self.tick((now - last_tick).as_secs_f64());
                    last_tick = now;
                }
            }
        }
        tracing::info!("zone stopped");
    }

    fn handle(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::Join { name, unicast_tx } => match &self.store {
                Some(store) => drop(store.send(StoreRequest::Load { name, unicast_tx })),
                None => self.admit(name, unicast_tx, None),
            },
            ClientEvent::Input { player_id, vx, vz, seq, t0 } => {
                let now = Instant::now();
                if let Some(player) = self.active_player(player_id) {
                    // ponytail: frames inside the gap are dropped, so a burst's final intent can be
                    // lost; fine for 10 Hz clients. Keep the latest dropped frame if clients send faster.
                    if player.last_input_at.is_none_or(|at| now - at >= MIN_INPUT_GAP) {
                        player.last_input_at = Some(now);
                        player.apply_input(vx, vz, seq, t0);
                    }
                }
            }
            // Active -> Suspended: frozen in place until the grace deadline.
            ClientEvent::Closed { player_id } => {
                let deadline = Instant::now() + self.grace;
                if let Some(player) = self.active_player(player_id) {
                    player.status = PlayerStatus::Suspended { deadline };
                }
            }
        }
    }

    /// Only Active players accept input or can become Suspended.
    fn active_player(&mut self, player_id: u64) -> Option<&mut Player> {
        self.players.get_mut(&player_id).filter(|player| player.status == PlayerStatus::Active)
    }

    /// Place a joining player in the world at its profile position (origin without one).
    fn admit(&mut self, name: String, unicast_tx: mpsc::Sender<ServerMsg>, profile: Option<Profile>) {
        let player_id = self.next_player_id;
        self.next_player_id += 1;
        let (x, z, yaw) = profile.map_or((0.0, 0.0, 0.0), |p| (p.x, p.z, p.yaw));
        let mut player = Player::new(player_id, name, x, z, yaw);
        player.profile_id = profile.map(|p| p.id);

        let joined = ServerMsg::Joined { player_id, name: player.name.clone(), x, z, yaw };
        // Never block the tick on one client. If the reply can't be delivered the
        // connection can never learn its id (so can't send Closed): don't keep a ghost.
        if unicast_tx.try_send(joined).is_ok() {
            let announce = ServerMsg::PlayerJoined { id: player_id, name: player.name.clone() };
            self.players.insert(player_id, player);
            drop(self.outbound.send(Outbound::Event(announce)));
        }
    }

    /// Suspended -> Removed once the grace deadline passes: persist, drop from
    /// the world, and tell everyone.
    fn remove_expired(&mut self) {
        let now = Instant::now();
        let expired: Vec<u64> = self
            .players
            .values()
            .filter(|player| matches!(player.status, PlayerStatus::Suspended { deadline } if deadline <= now))
            .map(|player| player.player_id)
            .collect();
        for id in expired {
            if let Some(player) = self.players.remove(&id) {
                self.persist(&player);
                drop(self.outbound.send(Outbound::Event(ServerMsg::PlayerLeft { id })));
            }
        }
    }

    fn persist(&self, player: &Player) {
        if let (Some(store), Some(profile_id)) = (&self.store, player.profile_id) {
            let save = StoreRequest::Save { profile_id, x: player.x, z: player.z, yaw: player.yaw };
            drop(store.send(save));
        }
    }

    fn tick(&mut self, dt: f64) {
        self.remove_expired();
        for player in self.players.values_mut() {
            if player.status == PlayerStatus::Active {
                player.integrate(dt, self.speed, self.world_half);
            }
        }

        let players: Vec<SnapshotPlayer> = self
            .players
            .values()
            .filter(|player| player.status != PlayerStatus::Removed)
            .map(Player::snapshot)
            .collect();
        let count = players.len();

        match serde_json::to_vec(&ServerMsg::Snapshot { players }) {
            // A send error only means nobody is subscribed right now.
            Ok(bytes) => drop(self.outbound.send(Outbound::Snapshot(bytes.into()))),
            Err(err) => tracing::error!(%err, "snapshot serialization failed"),
        }
        self.metrics.record_tick(count, dt);
    }
}

#[cfg(test)]
mod tests {
    use protocol::PlayerState;
    use tokio::time::timeout_at;

    use super::*;

    fn test_config() -> Config {
        Config {
            bind: String::new(),
            tick_hz: 20,
            grace_ms: 5000,
            speed: 5.0,
            world_half: 50.0,
            database_url: None,
        }
    }

    async fn join(zone: &ZoneHandle, name: &str) -> u64 {
        let (unicast_tx, mut unicast_rx) = mpsc::channel(1);
        let event = ClientEvent::Join { name: name.to_owned(), unicast_tx };
        zone.events.send(event).await.unwrap();
        match unicast_rx.recv().await {
            Some(ServerMsg::Joined { player_id, .. }) => player_id,
            other => panic!("expected joined, got {other:?}"),
        }
    }

    fn parse(bytes: &[u8]) -> Vec<SnapshotPlayer> {
        match serde_json::from_slice(bytes).unwrap() {
            ServerMsg::Snapshot { players } => players,
            other => panic!("expected snapshot, got {other:?}"),
        }
    }

    /// Receive snapshots until `done` accepts one, or panic after `within`.
    async fn wait_for_snapshot(
        rx: &mut broadcast::Receiver<Outbound>,
        within: Duration,
        done: impl Fn(&[SnapshotPlayer]) -> bool,
    ) -> Vec<SnapshotPlayer> {
        let deadline = Instant::now() + within;
        loop {
            match timeout_at(deadline, rx.recv()).await {
                Ok(Ok(Outbound::Snapshot(bytes))) => {
                    let players = parse(&bytes);
                    if done(&players) {
                        return players;
                    }
                }
                Ok(other) => panic!("unexpected outbound: {other:?}"),
                Err(_) => panic!("no matching snapshot within {within:?}"),
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn broadcasts_one_snapshot_per_tick_with_all_players() {
        let zone = spawn(&test_config(), None);
        let alice = join(&zone, "Alice").await;
        let bob = join(&zone, "Bob").await;
        assert!(bob > alice, "ids are monotonic");

        let mut snapshots = zone.outbound.subscribe();
        // Deadline sits between ticks (50 ms apart) so no tick ties with the timeout:
        // ticks at +50..=+500 ms, plus possibly the interval's immediate first tick.
        let deadline = Instant::now() + Duration::from_millis(525);
        let mut count = 0;
        while let Ok(frame) = timeout_at(deadline, snapshots.recv()).await {
            let Outbound::Snapshot(bytes) = frame.unwrap() else { panic!("unexpected event") };
            assert_eq!(parse(&bytes).len(), 2);
            count += 1;
        }
        assert!((10..=11).contains(&count), "got {count} snapshots in ~500 ms");

        let stats = zone.metrics.snapshot();
        assert_eq!(stats.players, 2);
        assert!(stats.tick_count >= 10);
        assert!((stats.last_tick_dt_ms - 50.0).abs() < 1.0, "dt {}", stats.last_tick_dt_ms);
    }

    #[tokio::test(start_paused = true)]
    async fn close_suspends_for_grace_then_removes_with_player_left() {
        let grace = Duration::from_millis(200);
        let zone = spawn(&Config { grace_ms: 200, ..test_config() }, None);
        let alice = join(&zone, "Alice").await;
        let bob = join(&zone, "Bob").await;
        let mut snapshots = zone.outbound.subscribe();
        let within = Duration::from_millis(500);

        let input = ClientEvent::Input { player_id: alice, vx: 1.0, vz: 0.0, seq: 1, t0: 42 };
        zone.events.send(input).await.unwrap();
        let players = wait_for_snapshot(&mut snapshots, within, |players| {
            players.iter().any(|p| p.id == alice && p.x > 0.0)
        })
        .await;
        let moved = players.iter().find(|p| p.id == alice).unwrap();
        assert_eq!((moved.state, moved.seq, moved.t0), (PlayerState::Walk, 1, 42));

        // Walking Alice disconnects: still broadcast, frozen and idle, and deaf to input.
        zone.events.send(ClientEvent::Closed { player_id: alice }).await.unwrap();
        let closed_at = Instant::now();
        let input = ClientEvent::Input { player_id: alice, vx: 0.0, vz: 1.0, seq: 2, t0: 43 };
        zone.events.send(input).await.unwrap();
        let players = wait_for_snapshot(&mut snapshots, within, |players| {
            players.iter().any(|p| p.id == alice && p.state == PlayerState::Idle)
        })
        .await;
        let frozen = players.iter().find(|p| p.id == alice).unwrap().clone();
        assert_eq!(frozen.seq, 1, "input while suspended is ignored");

        let left_at = loop {
            match timeout_at(closed_at + within, snapshots.recv()).await.unwrap().unwrap() {
                Outbound::Event(ServerMsg::PlayerLeft { id }) => {
                    assert_eq!(id, alice);
                    break Instant::now();
                }
                Outbound::Snapshot(bytes) => {
                    let players = parse(&bytes);
                    let alice = players.iter().find(|p| p.id == alice).expect("removed before playerLeft");
                    assert_eq!((alice.x, alice.z, alice.state), (frozen.x, frozen.z, PlayerState::Idle));
                }
                other => panic!("unexpected outbound: {other:?}"),
            }
        };
        assert!(left_at - closed_at >= grace, "removed after {:?}", left_at - closed_at);

        let players = wait_for_snapshot(&mut snapshots, within, |_| true).await;
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].id, bob);
    }

    #[tokio::test(start_paused = true)]
    async fn join_announces_player_joined() {
        let zone = spawn(&test_config(), None);
        let mut outbound = zone.outbound.subscribe();
        let alice = join(&zone, "Alice").await;
        let deadline = Instant::now() + Duration::from_millis(100);
        let event = loop {
            match timeout_at(deadline, outbound.recv()).await.expect("no playerJoined").unwrap() {
                Outbound::Event(event) => break event,
                Outbound::Snapshot(_) => continue,
            }
        };
        assert_eq!(event, ServerMsg::PlayerJoined { id: alice, name: "Alice".to_owned() });
    }

    #[tokio::test(start_paused = true)]
    async fn inputs_inside_the_rate_cap_gap_are_dropped() {
        let zone = spawn(&test_config(), None);
        let alice = join(&zone, "Alice").await;
        let mut snapshots = zone.outbound.subscribe();

        // Same instant: only seq 1 is accepted.
        for seq in 1..=3 {
            let input = ClientEvent::Input { player_id: alice, vx: 1.0, vz: 0.0, seq, t0: seq };
            zone.events.send(input).await.unwrap();
        }
        tokio::time::sleep(MIN_INPUT_GAP).await;
        let input = ClientEvent::Input { player_id: alice, vx: 0.0, vz: 0.0, seq: 4, t0: 4 };
        zone.events.send(input).await.unwrap();

        wait_for_snapshot(&mut snapshots, Duration::from_millis(500), |players| {
            let seq = players[0].seq;
            assert!(matches!(seq, 0 | 1 | 4), "rate-capped input seq {seq} was accepted");
            seq == 4
        })
        .await;
    }
}
