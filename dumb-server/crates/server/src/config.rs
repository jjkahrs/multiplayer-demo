//! Runtime configuration, sourced from environment variables with known
//! fallbacks. Kept in one place so every subsystem reads the same values.

use std::env;

/// Effective server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Socket address to bind, `BIND` (default `0.0.0.0:8080`).
    pub bind: String,
    /// Zone simulation ticks per second, `TICK_HZ` (default 20).
    pub tick_hz: u64,
    /// Grace period for a disconnected player before removal (ms), `GRACE_MS`
    /// (default 5000).
    pub grace_ms: u64,
    /// Player walk speed in meters/second, `SPEED` (default 5.0).
    pub speed: f64,
    /// Half-extent of the square world in meters, `WORLD_HALF` (default 50).
    pub world_half: f64,
    /// MySQL URL, `DATABASE_URL`. Absent or empty: run without persistence.
    pub database_url: Option<String>,
}

impl Config {
    /// Load configuration from the environment, applying built-in defaults
    /// when a variable is absent or unparseable.
    pub fn from_env() -> Self {
        Self {
            bind: env_or("BIND", "0.0.0.0:8080"),
            tick_hz: env_parse("TICK_HZ", 20),
            grace_ms: env_parse("GRACE_MS", 5000),
            speed: env_parse("SPEED", 5.0),
            world_half: env_parse("WORLD_HALF", 50.0),
            database_url: env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.is_empty()),
        }
    }

    /// Log the effective values once at startup so a run is reproducible.
    pub fn log(&self) {
        tracing::info!(
            bind = %self.bind,
            tick_hz = self.tick_hz,
            grace_ms = self.grace_ms,
            speed = self.speed,
            world_half = self.world_half,
            database_url = self.database_url.as_deref().map_or("(none)".to_owned(), redact_password),
            "effective config"
        );
    }
}

/// `scheme://user:password@host/...` -> `scheme://user:***@host/...`.
fn redact_password(url: &str) -> String {
    let Some(creds_start) = url.find("://").map(|i| i + 3) else {
        return url.to_owned();
    };
    let Some(at) = url[creds_start..].rfind('@').map(|i| creds_start + i) else {
        return url.to_owned();
    };
    match url[creds_start..at].find(':') {
        Some(colon) => format!("{}:***{}", &url[..creds_start + colon], &url[at..]),
        None => url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::redact_password;

    #[test]
    fn redacts_only_the_password() {
        assert_eq!(redact_password("mysql://demo:demo@mysql:3306/demo"), "mysql://demo:***@mysql:3306/demo");
        assert_eq!(redact_password("mysql://demo:p@ss@host/db"), "mysql://demo:***@host/db");
        assert_eq!(redact_password("mysql://demo@host/db"), "mysql://demo@host/db");
        assert_eq!(redact_password("mysql://host:3306/db"), "mysql://host:3306/db");
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(default)
}