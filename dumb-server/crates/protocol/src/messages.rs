//! WebSocket message types exchanged between client and server.
//!
//! Wire format is JSON with camelCase field names. Every message carries a
//! `type` tag whose value is the lowercase message kind (`join`, `input`,
//! `leave`, `joined`, `snapshot`, etc.).
//!
//! Numeric conventions (consistent across all messages):
//! - `x`, `z`, `yaw`, `vx`, `vz` are `f64`. Positions are meters, `yaw` is
//!   radians, velocities are normalized to `[-1, 1]`.
//! - Player/database ids are `u64`.
//! - `seq` (per-player input sequence) and `t0` (client clock, milliseconds)
//!   are `u64`. `t0` is a local timestamp; on this field's meaning see the
//!   technical design ("t0 echo").

use serde::{Deserialize, Serialize};

/// Animation state of a player as rendered by clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerState {
    /// Player is not moving.
    Idle,
    /// Player is moving; the direction vector is authoritative over `yaw`.
    Walk,
}

/// One player's entry inside a snapshot.
///
/// Deliberately shaped as `id`/`name` (not `playerId`), unlike [`ServerMsg::Joined`] —
/// see the technical design ("separate the snapshot entry from joined shaping").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPlayer {
    pub id: u64,
    pub name: String,
    pub x: f64,
    pub z: f64,
    pub yaw: f64,
    pub state: PlayerState,
    pub seq: u64,
    pub t0: u64,
    /// Milliseconds the server has integrated the current direction, through input `seq`
    /// (sum of tick dt since an accepted input last changed the direction).
    pub age_ms: u64,
}

/// Messages a client sends to the server.
///
/// Untrusted input: the server validates names and ignores out-of-range
/// velocity vectors (see the design's trust-level decision).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ClientMsg {
    /// Request to enter the world under a display name.
    Join {
        name: String,
    },
    /// Movement intent. `vx`/`vz` must be in `[-1, 1]`; anything else is
    /// ignored by the server. `seq` is monotonic per player, `t0` a local
    /// millisecond timestamp for one-way latency measurement.
    Input {
        vx: f64,
        vz: f64,
        seq: u64,
        t0: u64,
    },
    /// Explicit disconnect intent; otherwise a socket close implies it.
    Leave,
}

/// Messages the server sends to a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ServerMsg {
    /// Reply to a successful `join`, sent once to that client only: its
    /// session id and initial authoritative position/facing, plus the
    /// movement rules (`speed`, `world_half`, `tick_hz`) for client prediction.
    Joined {
        player_id: u64,
        name: String,
        x: f64,
        z: f64,
        yaw: f64,
        speed: f64,
        world_half: f64,
        tick_hz: u64,
    },
    /// Full world snapshot broadcast at the tick rate, containing every
    /// connected (non-removed) player including the receiver.
    Snapshot {
        /// Zone tick counter: monotonic, starts at 0.
        tick: u64,
        players: Vec<SnapshotPlayer>,
    },
    /// A player entered the world while this client was connected.
    PlayerJoined {
        id: u64,
        name: String,
    },
    /// A player left the world while this client was connected.
    PlayerLeft {
        id: u64,
    },
    /// An error reply: malformed input, bad name, etc. The `code` is a stable
    /// machine-readable string (e.g. `bad_name`, `bad_message`).
    #[serde(rename = "error")]
    ErrorMsg {
        code: String,
        message: String,
    },
}