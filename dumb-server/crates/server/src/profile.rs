//! MySQL profile repository: display name + last position. Runtime-checked
//! queries only, so builds never need a live database.

use std::f64::consts::TAU;
use std::hash::{BuildHasher, RandomState};

use sqlx::MySqlPool;

/// Brand-new profiles spawn within this distance of the origin (so bots don't stack).
const NEW_SPAWN_RADIUS: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub id: u64,
    pub x: f64,
    pub z: f64,
    pub yaw: f64,
}

/// The most recently updated profile for `name`, or a new one near the origin.
/// Names match under the column's collation (case- and accent-insensitive).
pub async fn load_or_create(pool: &MySqlPool, name: &str) -> sqlx::Result<Profile> {
    let existing: Option<(u64, f64, f64, f64)> = sqlx::query_as(
        "SELECT id, pos_x, pos_z, yaw FROM profiles WHERE display_name = ? \
         ORDER BY updated_at DESC, id DESC LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    if let Some((id, x, z, yaw)) = existing {
        return Ok(Profile { id, x, z, yaw });
    }

    let (x, z, yaw) = new_spawn();
    let id = sqlx::query("INSERT INTO profiles (display_name, pos_x, pos_z, yaw) VALUES (?, ?, ?, ?)")
        .bind(name)
        .bind(x)
        .bind(z)
        .bind(yaw)
        .execute(pool)
        .await?
        .last_insert_id();
    Ok(Profile { id, x, z, yaw })
}

pub async fn save(pool: &MySqlPool, profile_id: u64, x: f64, z: f64, yaw: f64) -> sqlx::Result<()> {
    sqlx::query("UPDATE profiles SET pos_x = ?, pos_z = ?, yaw = ? WHERE id = ?")
        .bind(x)
        .bind(z)
        .bind(yaw)
        .bind(profile_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A random point within [`NEW_SPAWN_RADIUS`] of the origin, facing the origin.
fn new_spawn() -> (f64, f64, f64) {
    let radius = NEW_SPAWN_RADIUS * random_unit();
    let angle = TAU * random_unit();
    let (x, z) = (radius * angle.cos(), radius * angle.sin());
    (x, z, (-z).atan2(-x))
}

// ponytail: stdlib's randomly keyed hasher as an RNG; plenty for spawn scatter, add `rand` if quality matters.
/// Uniform in `[0, 1)`.
fn random_unit() -> f64 {
    (RandomState::new().hash_one(()) >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_spawn_is_near_origin_and_faces_it() {
        for _ in 0..1000 {
            let (x, z, yaw) = new_spawn();
            assert!(x.hypot(z) < NEW_SPAWN_RADIUS);
            if x.hypot(z) > 1e-6 {
                let (face_x, face_z) = (yaw.cos(), yaw.sin());
                assert!((face_x * -x + face_z * -z) / x.hypot(z) > 0.999, "yaw {yaw} at ({x},{z})");
            }
        }
    }
}
