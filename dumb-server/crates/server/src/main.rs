//! Server entry point: tracing setup, config, router, bind, graceful shutdown.

use std::process::exit;
use std::time::Duration;

use sqlx::mysql::MySqlPoolOptions;
use tokio::net::TcpListener;
use tracing::info;

use server::config::Config;
use server::http::router;
use server::metrics::Metrics;
use server::zone;

const METRICS_LOG_PERIOD: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();
    config.log();

    let pool = match &config.database_url {
        // Lazy: the server starts even if MySQL is still coming up; joins degrade until it is.
        Some(url) => match MySqlPoolOptions::new().connect_lazy(url) {
            Ok(pool) => {
                info!("profile persistence enabled");
                Some(pool)
            }
            Err(err) => {
                tracing::error!(%err, "invalid DATABASE_URL");
                exit(1);
            }
        },
        None => {
            tracing::warn!("DATABASE_URL not set: running without persistence");
            None
        }
    };

    let zone = zone::spawn(&config, pool);
    tokio::spawn(log_metrics(zone.metrics.clone()));
    let app = router(zone);
    let listener = match TcpListener::bind(&config.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!(bind = %config.bind, %err, "failed to bind");
            exit(1);
        }
    };
    info!(bind = %config.bind, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server failed");

    info!("server stopped");
}

/// Log one `[metrics]` line every [`METRICS_LOG_PERIOD`].
async fn log_metrics(metrics: Metrics) {
    let mut period = tokio::time::interval(METRICS_LOG_PERIOD);
    loop {
        period.tick().await;
        let s = metrics.snapshot();
        info!(
            "[metrics] players={} tick_dt_ms={:.1} snapshots_per_sec={:.1} cpu={:.1}% mem_mb={:.1}",
            s.players,
            s.last_tick_dt_ms,
            s.snapshots_per_sec,
            s.cpu_percent,
            s.mem_bytes as f64 / (1024.0 * 1024.0),
        );
    }
}

/// Wait for ctrl-c, then log the graceful shutdown that `axum::serve` begins.
async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::warn!(%err, "failed to listen for shutdown signal");
    }
    info!("shutting down");
}
