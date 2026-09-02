//! Deterministic local provider, artifact, queue, timing, and resource observations.

#[cfg(target_os = "linux")]
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use peritus_benchmarks::{Metric, QueueKind};

use crate::SubjectError;

pub fn append_artifact(path: &Path, bytes: usize) -> Result<(), SubjectError> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let chunk = [0xA5_u8; 16 * 1024];
    let mut remaining = bytes;
    while remaining != 0 {
        let count = remaining.min(chunk.len());
        file.write_all(&chunk[..count])?;
        remaining -= count;
    }
    file.flush()?;
    file.sync_data()?;
    Ok(())
}

pub fn deterministic_provider_chunk(request: u64, tokens: u64) -> u64 {
    let rounds = tokens.min(65_536);
    (0..rounds).fold(request, |state, token| state.rotate_left(7) ^ token.wrapping_mul(0x9E37_79B9))
}

pub const fn queue_metric(queue: QueueKind) -> Metric {
    match queue {
        QueueKind::Command => Metric::CommandQueueDepth,
        QueueKind::Terminal => Metric::TerminalQueueDepth,
        QueueKind::Exporter => Metric::ExporterQueueDepth,
        QueueKind::Provider => Metric::ProviderQueueDepth,
    }
}

pub const fn backpressure_metric(queue: QueueKind) -> Metric {
    match queue {
        QueueKind::Command | QueueKind::Terminal => Metric::QueueSaturationWait,
        QueueKind::Exporter => Metric::ExporterBackpressureLatency,
        QueueKind::Provider => Metric::ProviderBackpressureLatency,
    }
}

pub fn throughput(units: u64, duration: Duration) -> u64 {
    let elapsed = u128::from(micros(duration).max(1));
    let rate = u128::from(units).saturating_mul(1_000_000) / elapsed;
    u64::try_from(rate).unwrap_or(u64::MAX)
}

pub fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

#[cfg(target_os = "linux")]
pub fn resident_bytes(pid: u32) -> Result<u64, SubjectError> {
    let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| std::io::Error::other("daemon status did not contain VmRSS"))?;
    kib.checked_mul(1024).ok_or(SubjectError::IdentityExhausted)
}

#[cfg(not(target_os = "linux"))]
pub const fn resident_bytes(_pid: u32) -> Result<u64, SubjectError> {
    Err(SubjectError::UnsupportedPlatform)
}
