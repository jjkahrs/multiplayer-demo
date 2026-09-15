//! Player entity owned by the Zone: position, movement intent, and the
//! lifecycle FSM `Joining -> Active -> Suspended -> Removed`.

use protocol::{PlayerState, SnapshotPlayer};
use tokio::time::Instant;

/// Below this magnitude a direction counts as "not moving".
const MOVE_EPSILON: f64 = 1e-6;

/// Lifecycle state of a player inside the Zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatus {
    /// Accepted but not yet placed in the world (profile load lands in T3.2).
    Joining,
    /// In the world, simulated and broadcast.
    Active,
    /// Socket lost; frozen in place (broadcast as idle) until `deadline`.
    Suspended { deadline: Instant },
    /// Out of the world; never simulated or broadcast.
    Removed,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub player_id: u64,
    pub name: String,
    pub x: f64,
    pub z: f64,
    /// Facing in radians, `atan2(dir_z, dir_x)` of the last movement.
    pub yaw: f64,
    pub dir_x: f64,
    pub dir_z: f64,
    /// Newest accepted input sequence, echoed in snapshots.
    pub seq: u64,
    /// Newest accepted input client timestamp (ms), echoed in snapshots.
    pub t0: u64,
    /// Seconds the Zone has integrated the current direction. Only a direction change resets it, so
    /// same-direction resends don't re-anchor it to a tick boundary; clients use it to skip simulated time.
    pub input_age: f64,
    pub status: PlayerStatus,
    /// Database profile row, `None` when running without persistence.
    pub profile_id: Option<u64>,
    /// When the last input passed the Zone's rate cap.
    pub last_input_at: Option<Instant>,
}

impl Player {
    /// A player placed in the world at the given position, standing still.
    pub fn new(player_id: u64, name: String, x: f64, z: f64, yaw: f64) -> Self {
        Self {
            player_id,
            name,
            x,
            z,
            yaw,
            dir_x: 0.0,
            dir_z: 0.0,
            seq: 0,
            t0: 0,
            input_age: 0.0,
            status: PlayerStatus::Active,
            profile_id: None,
            last_input_at: None,
        }
    }

    /// Accept a movement intent. Vectors longer than 1 (or non-finite) are
    /// untrusted and ignored entirely, leaving direction, `seq`, `t0` and `input_age` as-is.
    pub fn apply_input(&mut self, vx: f64, vz: f64, seq: u64, t0: u64) {
        let magnitude = vx.hypot(vz);
        if !magnitude.is_finite() || magnitude > 1.0 + MOVE_EPSILON {
            return;
        }
        let direction = if magnitude > MOVE_EPSILON {
            (vx / magnitude, vz / magnitude)
        } else {
            (0.0, 0.0)
        };
        if direction != (self.dir_x, self.dir_z) {
            self.input_age = 0.0;
        }
        (self.dir_x, self.dir_z) = direction;
        self.seq = seq;
        self.t0 = t0;
    }

    /// Advance position by `dt` seconds, clamped to the square world.
    pub fn integrate(&mut self, dt: f64, speed: f64, world_half: f64) {
        if !self.is_moving() {
            return;
        }
        self.x = (self.x + self.dir_x * speed * dt).clamp(-world_half, world_half);
        self.z = (self.z + self.dir_z * speed * dt).clamp(-world_half, world_half);
        self.yaw = self.dir_z.atan2(self.dir_x);
    }

    /// Animation state: only an Active player with a direction walks.
    pub fn state(&self) -> PlayerState {
        if self.status == PlayerStatus::Active && self.is_moving() {
            PlayerState::Walk
        } else {
            PlayerState::Idle
        }
    }

    pub fn snapshot(&self) -> SnapshotPlayer {
        SnapshotPlayer {
            id: self.player_id,
            name: self.name.clone(),
            x: self.x,
            z: self.z,
            yaw: self.yaw,
            state: self.state(),
            seq: self.seq,
            t0: self.t0,
            age_ms: (self.input_age * 1000.0).round() as u64,
        }
    }

    fn is_moving(&self) -> bool {
        self.dir_x.hypot(self.dir_z) > MOVE_EPSILON
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::*;

    const SPEED: f64 = 5.0;
    const HALF: f64 = 50.0;

    fn player_at(x: f64, z: f64) -> Player {
        Player::new(1, "Test".to_owned(), x, z, 0.0)
    }

    #[test]
    fn clamps_to_world_bounds() {
        let mut player = player_at(49.0, -49.0);
        player.apply_input(1.0, 0.0, 1, 0);
        player.integrate(1.0, SPEED, HALF);
        assert_eq!(player.x, 50.0);

        player.apply_input(0.0, -1.0, 2, 0);
        player.integrate(1.0, SPEED, HALF);
        assert_eq!(player.z, -50.0);

        player.apply_input(-1.0, 0.0, 3, 0);
        player.integrate(1000.0, SPEED, HALF);
        assert_eq!(player.x, -50.0);
    }

    #[test]
    fn yaw_follows_direction() {
        let cases = [(1.0, 0.0, 0.0), (0.0, 1.0, FRAC_PI_2), (-1.0, 0.0, PI), (0.0, -1.0, -FRAC_PI_2)];
        for (vx, vz, expected) in cases {
            let mut player = player_at(0.0, 0.0);
            player.apply_input(vx, vz, 1, 0);
            player.integrate(0.05, SPEED, HALF);
            assert!((player.yaw - expected).abs() < 1e-9, "({vx},{vz}) -> {}", player.yaw);
        }
    }

    #[test]
    fn out_of_range_input_is_ignored() {
        let mut player = player_at(3.0, 4.0);
        player.apply_input(1.5, 0.0, 7, 99);
        player.apply_input(0.9, 0.9, 8, 99);
        player.apply_input(f64::NAN, 0.0, 9, 99);
        player.integrate(1.0, SPEED, HALF);
        assert_eq!((player.x, player.z), (3.0, 4.0));
        assert_eq!((player.seq, player.t0), (0, 0));
        assert_eq!(player.state(), PlayerState::Idle);
    }

    #[test]
    fn input_age_resets_on_direction_change_only() {
        let mut player = player_at(0.0, 0.0);
        player.input_age = 1.0;
        player.apply_input(1.5, 0.0, 1, 0);
        assert_eq!(player.input_age, 1.0, "rejected input keeps the age");
        player.apply_input(0.0, 0.0, 2, 0);
        assert_eq!((player.input_age, player.seq), (1.0, 2), "same direction keeps the age");
        player.apply_input(1.0, 0.0, 3, 0);
        assert_eq!(player.input_age, 0.0, "new direction resets the age");
        player.input_age = 0.5;
        player.apply_input(1.0, 0.0, 4, 0);
        assert_eq!((player.input_age, player.seq), (0.5, 4), "resend keeps the age");
        assert_eq!(player.snapshot().age_ms, 500);
    }

    #[test]
    fn state_derives_from_movement_and_status() {
        let mut player = player_at(0.0, 0.0);
        assert_eq!(player.state(), PlayerState::Idle);

        player.apply_input(0.6, 0.8, 1, 0);
        assert_eq!(player.state(), PlayerState::Walk);

        player.status = PlayerStatus::Suspended { deadline: Instant::now() };
        assert_eq!(player.state(), PlayerState::Idle);

        player.status = PlayerStatus::Active;
        player.apply_input(0.0, 0.0, 2, 0);
        assert_eq!(player.state(), PlayerState::Idle);
    }
}
