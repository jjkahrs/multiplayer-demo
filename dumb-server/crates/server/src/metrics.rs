//! Zone statistics: written by the Zone once per tick, read by `/metrics` and
//! the periodic log. Process CPU/RAM is sampled on each read.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::time::Instant;

/// Window over which `snapshots_per_sec` is averaged.
const RATE_WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub players: u32,
    pub tick_count: u64,
    /// Wall time between the last two ticks, in milliseconds.
    pub last_tick_dt_ms: f64,
    /// Snapshots broadcast per second over the last completed window.
    pub snapshots_per_sec: f64,
    /// Server process CPU; 100 = one full core, so it can exceed 100 on multi-core.
    pub cpu_percent: f32,
    /// Server process resident memory.
    pub mem_bytes: u64,
}

#[derive(Default)]
struct ZoneStats {
    stats: Stats,
    window_start: Option<Instant>,
    window_ticks: u64,
}

struct ProcessProbe {
    system: System,
    pid: Option<Pid>,
}

/// Cheaply cloneable shared handle to [`Stats`].
///
/// Zone stats and the process probe sit behind separate locks so a slow OS
/// query on read never delays a tick.
#[derive(Clone)]
pub struct Metrics {
    zone: Arc<Mutex<ZoneStats>>,
    process: Arc<Mutex<ProcessProbe>>,
}

impl Default for Metrics {
    fn default() -> Self {
        let probe = ProcessProbe { system: System::new(), pid: sysinfo::get_current_pid().ok() };
        Self { zone: Arc::default(), process: Arc::new(Mutex::new(probe)) }
    }
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let zone = self.zone.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_tuple("Metrics").field(&zone.stats).finish()
    }
}

impl Metrics {
    pub fn record_tick(&self, players: usize, dt_secs: f64) {
        let mut zone = self.zone.lock().unwrap_or_else(PoisonError::into_inner);
        zone.stats.players = players as u32;
        zone.stats.tick_count += 1;
        zone.stats.last_tick_dt_ms = dt_secs * 1000.0;

        let now = Instant::now();
        match zone.window_start {
            None => zone.window_start = Some(now),
            Some(start) => {
                zone.window_ticks += 1;
                let elapsed = now - start;
                if elapsed >= RATE_WINDOW {
                    zone.stats.snapshots_per_sec = zone.window_ticks as f64 / elapsed.as_secs_f64();
                    zone.window_start = Some(now);
                    zone.window_ticks = 0;
                }
            }
        }
    }

    /// Current stats. CPU is measured since the previous call, so the first
    /// read after startup reports 0.
    pub fn snapshot(&self) -> Stats {
        let mut stats = self.zone.lock().unwrap_or_else(PoisonError::into_inner).stats;
        let mut probe = self.process.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(pid) = probe.pid {
            let refresh = ProcessRefreshKind::nothing().with_cpu().with_memory();
            probe.system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, refresh);
            if let Some(process) = probe.system.process(pid) {
                stats.cpu_percent = process.cpu_usage();
                stats.mem_bytes = process.memory();
            }
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn snapshots_per_sec_over_window() {
        let metrics = Metrics::default();
        for _ in 0..=40 {
            metrics.record_tick(3, 0.05);
            tokio::time::advance(Duration::from_millis(50)).await;
        }
        let stats = metrics.snapshot();
        assert_eq!(stats.snapshots_per_sec, 20.0);
        assert_eq!(stats.players, 3);
        assert!(stats.mem_bytes > 0, "process memory sampled");
    }
}
