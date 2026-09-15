//! Aggregate per-bot stats into the run report.

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

use crate::bot::BotStats;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub total: usize,
    pub connected: usize,
    pub dropped: usize,
    /// Average snapshots/s across connected bots.
    pub recv_rate_hz: f64,
    pub latency_samples: usize,
    pub latency_avg_ms: f64,
    pub latency_p95_ms: u64,
    /// Server `/metrics` at the end of the run, if reachable.
    pub server_metrics: Option<serde_json::Value>,
}

impl Report {
    pub fn build(stats: &[BotStats], server_metrics: Option<serde_json::Value>) -> Self {
        let connected: Vec<_> = stats.iter().filter(|s| s.connected).collect();
        let recv_rate_hz = mean(connected.iter().filter(|s| s.active_secs > 0.0).map(|s| s.snapshots as f64 / s.active_secs));
        let mut samples: Vec<u64> = connected.iter().flat_map(|s| s.latencies_ms.iter().copied()).collect();
        samples.sort_unstable();
        Self {
            total: stats.len(),
            connected: connected.len(),
            dropped: stats.iter().filter(|s| s.dropped).count(),
            recv_rate_hz,
            latency_samples: samples.len(),
            latency_avg_ms: mean(samples.iter().map(|&v| v as f64)),
            latency_p95_ms: percentile(&samples, 0.95),
            server_metrics,
        }
    }

    pub fn print(&self) {
        println!();
        println!("{:<22} {}/{} connected", "connections", self.connected, self.total);
        println!("{:<22} {}", "forced drops", self.dropped);
        println!("{:<22} {:.1} Hz", "recv rate (avg)", self.recv_rate_hz);
        println!("{:<22} {}", "latency samples", self.latency_samples);
        println!("{:<22} {:.1} ms", "latency avg", self.latency_avg_ms);
        println!("{:<22} {} ms", "latency p95", self.latency_p95_ms);
        match &self.server_metrics {
            Some(m) => println!("{:<22} {m}", "server /metrics"),
            None => println!("{:<22} unavailable", "server /metrics"),
        }
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    if n == 0 { 0.0 } else { sum / n as f64 }
}

/// Nearest-rank percentile of sorted samples; 0 when empty.
fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// GET `/metrics` on the host behind a `ws://host:port/...` URL. Raw HTTP/1.1
/// keeps the bot free of an HTTP client dependency.
pub async fn fetch_metrics(ws_url: &str) -> Option<serde_json::Value> {
    let host = ws_url.strip_prefix("ws://")?.split('/').next()?;
    timeout(Duration::from_secs(2), async {
        let mut stream = TcpStream::connect(host).await.ok()?;
        let request = format!("GET /metrics HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await.ok()?;
        if !response.starts_with("HTTP/1.1 200") {
            return None;
        }
        serde_json::from_str(response.split("\r\n\r\n").nth(1)?).ok()
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_nearest_rank() {
        let samples: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&samples, 0.95), 95);
        assert_eq!(percentile(&[7], 0.95), 7);
        assert_eq!(percentile(&[], 0.95), 0);
    }

    #[test]
    fn aggregates_connected_bots_only() {
        let stats = [
            BotStats { connected: true, snapshots: 200, active_secs: 10.0, latencies_ms: vec![10, 30], ..Default::default() },
            BotStats { connected: true, dropped: true, snapshots: 100, active_secs: 5.0, latencies_ms: vec![20], ..Default::default() },
            BotStats::default(),
        ];
        let report = Report::build(&stats, None);
        assert_eq!((report.total, report.connected, report.dropped), (3, 2, 1));
        assert_eq!(report.recv_rate_hz, 20.0);
        assert_eq!(report.latency_avg_ms, 20.0);
        assert_eq!(report.latency_p95_ms, 30);
    }
}
