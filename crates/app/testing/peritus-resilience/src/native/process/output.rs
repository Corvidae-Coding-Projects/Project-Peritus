//! Bounded readers for native controller stdout responses and stderr diagnostics.

use std::io::{self, BufRead as _, BufReader, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use crate::SubjectError;

use super::super::diagnostics::Diagnostics;
use super::supervision;

pub(super) fn read_responses(
    stdout: impl Read,
    maximum: u64,
    count: &AtomicU64,
    sender: &Sender<Result<Vec<u8>, SubjectError>>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        let read = reader.by_ref().take(maximum.saturating_add(1)).read_until(b'\n', &mut line);
        let bytes = match read {
            Ok(0) => return,
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = sender
                    .send(Err(supervision(format!("read controller response: {error}"), true)));
                return;
            }
        };
        count.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::AcqRel);
        if u64::try_from(line.len()).unwrap_or(u64::MAX) > maximum || !line.ends_with(b"\n") {
            let _ = sender.send(Err(supervision(
                "controller response exceeded its byte bound or lacked a newline",
                false,
            )));
            return;
        }
        line.pop();
        if line.is_empty() {
            let _ = sender.send(Err(supervision("controller returned an empty response", false)));
            return;
        }
        if sender.send(Ok(line)).is_err() {
            return;
        }
    }
}

pub(super) fn drain(
    mut reader: impl Read,
    count: &AtomicU64,
    diagnostics: &Diagnostics,
) -> io::Result<()> {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let bytes = reader.read(&mut buffer)?;
        if bytes == 0 {
            return Ok(());
        }
        count.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::AcqRel);
        diagnostics.record(&buffer[..bytes]);
    }
}
