use std::fs;

/// Process memory breakdown from /proc/self/status.
pub struct ProcessMemory {
    /// VmRSS: total resident set size (private + shared + file-backed)
    pub rss_bytes: u64,
    /// RssAnon: private anonymous pages (heap, stack) — the "real" cost
    pub anon_bytes: u64,
}

/// Read process memory from /proc/self/status.
/// Returns both VmRSS (total resident) and RssAnon (private only).
pub fn read_process_memory() -> Option<ProcessMemory> {
    let content = fs::read_to_string("/proc/self/status").ok()?;
    let mut rss: Option<u64> = None;
    let mut anon: Option<u64> = None;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            rss = rest.trim().trim_end_matches(" kB").trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix("RssAnon:") {
            anon = rest.trim().trim_end_matches(" kB").trim().parse().ok();
        }
        if rss.is_some() && anon.is_some() {
            break;
        }
    }

    Some(ProcessMemory {
        rss_bytes: rss? * 1024,
        anon_bytes: anon? * 1024,
    })
}

/// Read system memory from /proc/meminfo. Returns (total_bytes, available_bytes).
pub fn read_system_memory() -> Option<(u64, u64)> {
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    let mut total: Option<u64> = None;
    let mut available: Option<u64> = None;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = rest.trim().trim_end_matches(" kB").trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available = rest.trim().trim_end_matches(" kB").trim().parse().ok();
        }
        if total.is_some() && available.is_some() {
            break;
        }
    }

    let t = total? * 1024;
    let a = available? * 1024;
    Some((t, a))
}

/// Read process CPU ticks (utime + stime) from /proc/self/stat.
/// Returns total ticks consumed by the process.
pub fn read_process_cpu_ticks() -> Option<u64> {
    let content = fs::read_to_string("/proc/self/stat").ok()?;
    // Fields are space-separated. The comm field (2) may contain spaces inside parens.
    // Find the closing paren to skip comm, then parse remaining fields.
    let after_comm = content.split_once(')')?.1;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    // After closing paren: field index 0 = state (field 3 in proc stat)
    // utime = field 14 in stat = index 11 after paren (14 - 3)
    // stime = field 15 in stat = index 12 after paren (15 - 3)
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// Read total system CPU ticks from /proc/stat (first "cpu" line).
/// Returns total ticks across all CPUs.
pub fn read_system_cpu_ticks() -> Option<u64> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    let line = content.lines().find(|l| l.starts_with("cpu "))?;
    let total: u64 = line
        .split_whitespace()
        .skip(1) // skip "cpu" label
        .filter_map(|v| v.parse::<u64>().ok())
        .sum();
    Some(total)
}
