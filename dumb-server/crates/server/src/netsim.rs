//! Simulated network conditions: a per-connection, per-direction queue that
//! holds frames until a latency + jitter release time. Never reorders.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::time::Duration;

use tokio::time::{Instant, sleep_until};

/// Simulated network conditions. Round-trip figures; each direction gets half.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetSim {
    /// Added round-trip time, `LATENCY_MS`.
    pub latency_ms: u64,
    /// Max extra random round-trip time, `JITTER_MS`.
    pub jitter_ms: u64,
}

impl NetSim {
    /// True when both values are 0: callers bypass the queue entirely.
    pub fn is_off(self) -> bool {
        self.latency_ms == 0 && self.jitter_ms == 0
    }
}

/// Holds items until their release time; never reorders.
pub struct DelayQueue<T> {
    sim: NetSim,
    pending: VecDeque<(Instant, T)>,
    last_release: Instant,
    rng: u64,
}

impl<T> DelayQueue<T> {
    pub fn new(sim: NetSim) -> Self {
        Self {
            sim,
            pending: VecDeque::new(),
            last_release: Instant::now(),
            rng: RandomState::new().build_hasher().finish(),
        }
    }

    /// Schedule at `max(last_release, now + one_way_delay())`.
    pub fn push(&mut self, item: T) {
        let release = self.last_release.max(Instant::now() + self.one_way_delay());
        self.last_release = release;
        self.pending.push_back((release, item));
    }

    /// Wait until the head is due, then pop it. Pending forever when empty.
    /// Cancel-safe: pops only after the sleep completes, so losing a
    /// `select!` race never drops an item.
    pub async fn next(&mut self) -> T {
        let Some(&(release, _)) = self.pending.front() else {
            return std::future::pending().await;
        };
        sleep_until(release).await;
        self.pending.pop_front().expect("head checked above").1
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Half the round-trip figures: `latency/2 + uniform[0, jitter/2]`, in µs
    /// so odd millisecond values split exactly.
    fn one_way_delay(&mut self) -> Duration {
        let jitter_range = self.sim.jitter_ms.saturating_mul(500).saturating_add(1);
        let jitter = self.next_u64() % jitter_range;
        Duration::from_micros(self.sim.latency_ms.saturating_mul(500).saturating_add(jitter))
    }

    // ponytail: SplitMix64, statistical quality irrelevant for jitter; swap to rand if distributions matter
    fn next_u64(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{sleep, timeout};

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn zero_config_releases_immediately() {
        let mut q = DelayQueue::new(NetSim::default());
        let start = Instant::now();
        q.push(1);
        assert_eq!(q.next().await, 1);
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn delay_within_bounds() {
        let mut q = DelayQueue::new(NetSim { latency_ms: 100, jitter_ms: 40 });
        for i in 0..200 {
            let start = Instant::now();
            q.push(i);
            assert_eq!(q.next().await, i);
            let elapsed = start.elapsed();
            assert!(
                (Duration::from_millis(50)..=Duration::from_millis(70)).contains(&elapsed),
                "iteration {i}: {elapsed:?}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn preserves_order_under_jitter() {
        let mut q = DelayQueue::new(NetSim { latency_ms: 0, jitter_ms: 1000 });
        for i in 0..1000 {
            q.push(i);
        }
        let mut out = Vec::with_capacity(1000);
        for _ in 0..1000 {
            out.push(q.next().await);
        }
        assert_eq!(out, (0..1000).collect::<Vec<_>>());
    }

    #[tokio::test(start_paused = true)]
    async fn next_is_cancel_safe() {
        let mut q = DelayQueue::new(NetSim { latency_ms: 100, jitter_ms: 0 });
        q.push(7);
        assert!(timeout(Duration::from_millis(1), q.next()).await.is_err());
        assert_eq!(q.next().await, 7);
    }

    #[tokio::test(start_paused = true)]
    async fn idle_gap_does_not_accumulate() {
        let mut q = DelayQueue::new(NetSim { latency_ms: 100, jitter_ms: 0 });
        q.push(1);
        q.next().await;
        sleep(Duration::from_secs(1)).await;
        let start = Instant::now();
        q.push(2);
        assert_eq!(q.next().await, 2);
        assert_eq!(start.elapsed(), Duration::from_millis(50));
    }

    #[test]
    fn is_off_only_when_both_zero() {
        assert!(NetSim { latency_ms: 0, jitter_ms: 0 }.is_off());
        assert!(!NetSim { latency_ms: 1, jitter_ms: 0 }.is_off());
        assert!(!NetSim { latency_ms: 0, jitter_ms: 1 }.is_off());
    }
}
