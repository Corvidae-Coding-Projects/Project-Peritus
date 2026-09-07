//! Streaming, unsampled offered-load and completion evidence.

use std::fs::File;
use std::io::{BufWriter, Write as _};

use serde::Serialize;
use tempfile::NamedTempFile;

use crate::SubjectError;

/// Owned raw event-load evidence, retained independently of disposable daemon cleanup.
pub struct EventLoadEvidence {
    file: NamedTempFile,
}

impl EventLoadEvidence {
    pub(crate) fn path(&self) -> &std::path::Path {
        self.file.path()
    }
    /// Opens a fresh reader for every unsampled arrival, completion, and terminal summary.
    ///
    /// # Errors
    /// Returns the filesystem error if the private evidence file cannot be reopened.
    pub fn reader(&self) -> std::io::Result<File> {
        self.file.reopen()
    }
}

pub struct EvidenceWriter {
    file: NamedTempFile,
    writer: BufWriter<File>,
}

impl EvidenceWriter {
    pub fn new(directory: &std::path::Path) -> Result<Self, SubjectError> {
        let file = NamedTempFile::new_in(directory)?;
        let writer = BufWriter::new(file.reopen()?);
        Ok(Self { file, writer })
    }
    pub fn record(&mut self, value: &impl Serialize) -> Result<(), SubjectError> {
        serde_json::to_writer(&mut self.writer, value)?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<EventLoadEvidence, SubjectError> {
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        Ok(EventLoadEvidence { file: self.file })
    }
}

#[derive(Default, Serialize)]
pub struct Summary {
    pub expected: u64,
    pub offered: u64,
    pub schedule_missed: u64,
    pub queue_full: u64,
    pub expired: u64,
    pub committed: u64,
    pub failed: u64,
    pub committed_commands: u64,
    pub peak_active: usize,
    pub peak_queued: usize,
    pub elapsed_us: u64,
}

#[derive(Serialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum Observation<'a> {
    Arrival {
        sequence: u64,
        scheduled_us: u64,
        observed_us: u64,
        payload_bytes: u32,
        outcome: &'a str,
    },
    Expired {
        sequence: u64,
        observed_us: u64,
    },
    Completion(&'a super::worker::Completion),
    Summary {
        workload_id: &'a str,
        counters: &'a Summary,
    },
}
