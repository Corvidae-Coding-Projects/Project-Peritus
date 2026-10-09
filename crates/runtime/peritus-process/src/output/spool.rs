//! Bounded synchronized local stream spools.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

use crate::{ErrorCode, OutputStream, ProcessError, ProcessOperation, RecoveryClass};

pub(crate) struct BoundedSpool {
    file: File,
    limit: Option<u64>,
    written: u64,
}

impl BoundedSpool {
    fn create(
        directory: &Path,
        stream: OutputStream,
        limit: Option<u64>,
    ) -> Result<Self, ProcessError> {
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
        if self.limit.is_some_and(|limit| attempted > limit) {
            return Err(spool_error("stream spool byte limit exceeded"));
        }
        write_counted(&mut self.file, bytes, &mut self.written)
            .map_err(|_| spool_error("stream spool write failed"))
    }

    pub(crate) const fn written(&self) -> u64 {
        self.written
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
    pub(crate) fn pipes(directory: &Path, limit: Option<u64>) -> Result<Self, ProcessError> {
        Ok(Self {
            stdout: Some(BoundedSpool::create(directory, OutputStream::Stdout, limit)?),
            stderr: Some(BoundedSpool::create(directory, OutputStream::Stderr, limit)?),
            terminal: None,
        })
    }

    pub(crate) fn pty(directory: &Path, limit: Option<u64>) -> Result<Self, ProcessError> {
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

// `write_all` loses the accepted-prefix length on error. Keep each successful write visible
// so terminal accounting agrees with the actual spool even when the next write fails.
fn write_counted(
    writer: &mut impl Write,
    mut bytes: &[u8],
    written: &mut u64,
) -> std::io::Result<()> {
    while !bytes.is_empty() {
        match writer.write(bytes) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(count) => {
                *written += u64::try_from(count).map_err(std::io::Error::other)?;
                bytes = &bytes[count..];
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailedWriter {
        retained: Vec<u8>,
        available: usize,
    }
    impl Write for FailedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.available == 0 {
                return Err(std::io::Error::other("injected disk failure"));
            }
            let count = self.available.min(bytes.len());
            self.retained.extend_from_slice(&bytes[..count]);
            self.available -= count;
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn counted_write_preserves_exact_partial_progress_on_failure() {
        for available in [0, 3] {
            let mut writer = FailedWriter { retained: Vec::new(), available };
            let mut written = 0;
            assert!(write_counted(&mut writer, b"12345678", &mut written).is_err());
            assert_eq!(written, u64::try_from(available).expect("count"));
            assert_eq!(writer.retained, b"12345678"[..available]);
        }
    }
}
