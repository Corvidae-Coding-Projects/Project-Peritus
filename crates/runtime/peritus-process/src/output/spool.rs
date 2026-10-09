//! Bounded synchronized local stream spools.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

use crate::{ErrorCode, OutputStream, ProcessError, ProcessOperation, RecoveryClass};

pub(crate) struct BoundedSpool {
    file: File,
    limit: u64,
    written: u64,
}

impl BoundedSpool {
    fn create(directory: &Path, stream: OutputStream, limit: u64) -> Result<Self, ProcessError> {
        fs::create_dir_all(directory)
            .map_err(|_| spool_error("spool directory cannot be created"))?;
        let name = match stream {
            OutputStream::Stdout => "stdout.spool",
            OutputStream::Stderr => "stderr.spool",
            OutputStream::Terminal => "terminal.spool",
        };
        let path = directory.join(name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| spool_error("exclusive stream spool cannot be created"))?;
        Ok(Self { file, limit, written: 0 })
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<(), ProcessError> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| spool_error("spool chunk is unrepresentable"))?;
        let attempted = self
            .written
            .checked_add(length)
            .ok_or_else(|| spool_error("spool byte accounting overflowed"))?;
        if attempted > self.limit {
            return Err(spool_error("stream spool byte limit exceeded"));
        }
        self.file.write_all(bytes).map_err(|_| spool_error("stream spool write failed"))?;
        self.written = attempted;
        Ok(())
    }

    pub(crate) fn synchronize(&mut self) -> Result<(), ProcessError> {
        self.file
            .flush()
            .and_then(|()| self.file.sync_all())
            .map_err(|_| spool_error("stream spool cannot be synchronized"))
    }
}

pub(crate) struct SpoolSet {
    pub(crate) stdout: Option<BoundedSpool>,
    pub(crate) stderr: Option<BoundedSpool>,
    pub(crate) terminal: Option<BoundedSpool>,
}

impl SpoolSet {
    pub(crate) fn pipes(directory: &Path, limit: u64) -> Result<Self, ProcessError> {
        Ok(Self {
            stdout: Some(BoundedSpool::create(directory, OutputStream::Stdout, limit)?),
            stderr: Some(BoundedSpool::create(directory, OutputStream::Stderr, limit)?),
            terminal: None,
        })
    }

    pub(crate) fn pty(directory: &Path, limit: u64) -> Result<Self, ProcessError> {
        Ok(Self {
            stdout: None,
            stderr: None,
            terminal: Some(BoundedSpool::create(directory, OutputStream::Terminal, limit)?),
        })
    }
}

const fn spool_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Output,
        ProcessOperation::Stream,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}
