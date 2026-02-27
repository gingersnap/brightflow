use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time;

use super::proc;

/// Snapshot of system metrics at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct SystemSnapshot {
    /// VmRSS: total resident memory (the number that matters for capacity)
    pub process_rss_bytes: u64,
    /// RssAnon: private heap/stack only (what system monitors often show)
    pub process_anon_bytes: u64,
    pub system_used_bytes: u64,
    pub system_total_bytes: u64,
    pub cpu_percent: f64,
    pub uptime_secs: u64,
}

impl Default for SystemSnapshot {
    fn default() -> Self {
        Self {
            process_rss_bytes: 0,
            process_anon_bytes: 0,
            system_used_bytes: 0,
            system_total_bytes: 0,
            cpu_percent: 0.0,
            uptime_secs: 0,
        }
    }
}

/// Run the background sampler that reads /proc every 2 seconds.
pub async fn run_sampler(metrics: Arc<RwLock<SystemSnapshot>>, start_time: Instant) {
    let mut interval = time::interval(time::Duration::from_secs(2));

    // Track previous CPU ticks for delta calculation
    let mut prev_process_ticks = proc::read_process_cpu_ticks();
    let mut prev_system_ticks = proc::read_system_cpu_ticks();

    loop {
        interval.tick().await;

        let (process_rss_bytes, process_anon_bytes) = match proc::read_process_memory() {
            Some(mem) => (mem.rss_bytes, mem.anon_bytes),
            None => (0, 0),
        };

        let (system_total_bytes, system_used_bytes) = match proc::read_system_memory() {
            Some((total, available)) => (total, total.saturating_sub(available)),
            None => (0, 0),
        };

        // Calculate CPU % from tick deltas
        let cur_process_ticks = proc::read_process_cpu_ticks();
        let cur_system_ticks = proc::read_system_cpu_ticks();

        #[allow(clippy::cast_precision_loss)]
        let cpu_percent = match (
            prev_process_ticks,
            prev_system_ticks,
            cur_process_ticks,
            cur_system_ticks,
        ) {
            (Some(pp), Some(ps), Some(cp), Some(cs)) => {
                let process_delta = cp.saturating_sub(pp);
                let system_delta = cs.saturating_sub(ps);
                if system_delta > 0 {
                    (process_delta as f64 / system_delta as f64) * 100.0
                } else {
                    0.0
                }
            },
            _ => 0.0,
        };

        prev_process_ticks = cur_process_ticks;
        prev_system_ticks = cur_system_ticks;

        let uptime_secs = start_time.elapsed().as_secs();

        let snapshot = SystemSnapshot {
            process_rss_bytes,
            process_anon_bytes,
            system_used_bytes,
            system_total_bytes,
            cpu_percent,
            uptime_secs,
        };

        *metrics.write().await = snapshot;
    }
}
