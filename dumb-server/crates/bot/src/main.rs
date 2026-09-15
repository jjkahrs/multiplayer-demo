//! Bot loader: connects N headless players, random-walks them, measures
//! one-way A→server→B latency via the `t0` echo, and prints a report.

mod bot;
mod report;

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use tokio::time::Instant;

#[derive(Parser)]
#[command(about = "Headless load bots for the zone server")]
struct Cli {
    /// WebSocket endpoint of the server.
    #[arg(long, default_value = "ws://127.0.0.1:8080/ws")]
    server: String,
    /// Number of concurrent bots.
    #[arg(long, default_value_t = 1)]
    clients: usize,
    /// Run length in seconds.
    #[arg(long, default_value_t = 30)]
    duration: u64,
    /// Movement pattern.
    #[arg(long = "move", value_enum, default_value_t = MovePattern::Random)]
    movement: MovePattern,
    /// Also write the report as JSON to this path.
    #[arg(long)]
    json: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum MovePattern {
    Random,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let MovePattern::Random = cli.movement;
    let deadline = Instant::now() + Duration::from_secs(cli.duration);

    println!("starting {} bots against {} for {} s", cli.clients, cli.server, cli.duration);
    let handles: Vec<_> = (0..cli.clients)
        .map(|i| tokio::spawn(bot::run(i, cli.server.clone(), deadline)))
        .collect();
    let mut stats = Vec::with_capacity(handles.len());
    for handle in handles {
        stats.push(handle.await.expect("bot task panicked"));
    }

    let metrics = report::fetch_metrics(&cli.server).await;
    let report = report::Report::build(&stats, metrics);
    report.print();
    if let Some(path) = cli.json {
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap())
            .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
        println!("report written to {}", path.display());
    }
}
