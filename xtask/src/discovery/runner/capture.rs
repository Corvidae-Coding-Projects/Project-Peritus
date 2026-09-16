//! Concurrent bounded stdout and stderr capture for discovery children.

use crate::error::XtaskError;
use serde_json::json;
use std::fs::File;
use std::io::{self, Read, Write as _};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const ACTIVE: u8 = 0;
const CANCELLED: u8 = 1;
const COMMITTING: u8 = 2;

pub(super) struct CaptureSummary {
    observed_bytes: u64,
    retained_bytes: u64,
}

pub(super) struct CaptureTask {
    handle: JoinHandle<io::Result<CaptureSummary>>,
    state: Arc<AtomicU8>,
}

impl CaptureTask {
    pub(super) fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Claims cancellation before the capture has permission to write its bounded result.
    fn cancel(&self) -> bool {
        self.state.compare_exchange(ACTIVE, CANCELLED, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    pub(super) fn cancel_or_join(self) {
        if !self.cancel() {
            let _ = self.handle.join();
        }
    }
}

pub(super) fn finish(
    task: &mut Option<CaptureTask>,
    stream: &str,
    path: &Path,
    deadline: Instant,
) -> Result<CaptureSummary, XtaskError> {
    while task.as_ref().is_some_and(|task| !task.is_finished()) {
        if Instant::now() >= deadline {
            task.take().expect("capture remains owned").cancel_or_join();
            return Err(XtaskError::metadata(format!(
                "discovery {stream} pipe remained open after owned process cleanup"
            )));
        }
        thread::sleep(Duration::from_millis(20));
    }
    let task = task.take().ok_or_else(|| {
        XtaskError::metadata(format!("discovery {stream} capture ownership lost"))
    })?;
    task.handle
        .join()
        .map_err(|_| XtaskError::metadata(format!("discovery {stream} capture panicked")))?
        .map_err(|error| XtaskError::io(&format!("capture discovery {stream}"), path, error))
}

pub(super) fn spawn<R: Read + Send + 'static>(
    name: &str,
    reader: R,
    destination: File,
    limit: u64,
) -> Result<CaptureTask, XtaskError> {
    let state = Arc::new(AtomicU8::new(ACTIVE));
    let capture_state = Arc::clone(&state);
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || capture(reader, destination, limit, &capture_state))
        .map(|handle| CaptureTask { handle, state })
        .map_err(|error| XtaskError::metadata(format!("could not start {name} capture: {error}")))
}

fn capture<R: Read>(
    mut reader: R,
    mut destination: File,
    limit: u64,
    state: &AtomicU8,
) -> io::Result<CaptureSummary> {
    let limit = usize::try_from(limit)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "log limit does not fit usize"))?;
    let prefix_limit = limit / 2;
    let tail_limit = limit - prefix_limit;
    let mut observed = 0_u64;
    let mut prefix = Vec::with_capacity(prefix_limit);
    let mut tail = Vec::with_capacity(tail_limit);
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        observed = observed
            .checked_add(u64::try_from(count).expect("capture buffer length fits u64"))
            .ok_or_else(|| io::Error::other("observed log length overflowed"))?;
        let prefix_count = count.min(prefix_limit.saturating_sub(prefix.len()));
        prefix.extend_from_slice(&buffer[..prefix_count]);
        retain_tail(&mut tail, &buffer[prefix_count..count], tail_limit);
    }
    if state.compare_exchange(ACTIVE, COMMITTING, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        destination.write_all(&prefix)?;
        destination.write_all(&tail)?;
        destination.flush()?;
    }
    let retained = prefix
        .len()
        .checked_add(tail.len())
        .ok_or_else(|| io::Error::other("retained log length overflowed"))?;
    Ok(CaptureSummary {
        observed_bytes: observed,
        retained_bytes: u64::try_from(retained).expect("retained log limit fits u64"),
    })
}

fn retain_tail(tail: &mut Vec<u8>, bytes: &[u8], limit: usize) {
    if limit == 0 {
        return;
    }
    if bytes.len() >= limit {
        tail.clear();
        tail.extend_from_slice(&bytes[bytes.len() - limit..]);
        return;
    }
    let overflow = tail.len().saturating_add(bytes.len()).saturating_sub(limit);
    if overflow > 0 {
        tail.drain(..overflow);
    }
    tail.extend_from_slice(bytes);
}

pub(super) fn report(summary: &CaptureSummary, limit: u64) -> serde_json::Value {
    json!({
        "limit_bytes": limit,
        "observed_bytes": summary.observed_bytes,
        "retained_bytes": summary.retained_bytes,
        "truncated": summary.observed_bytes > summary.retained_bytes,
        "retention": "prefix_and_tail",
    })
}
