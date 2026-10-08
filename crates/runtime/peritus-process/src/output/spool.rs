//! Segmented synchronized local stream spools.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use crate::{ErrorCode, OutputStream, ProcessError, ProcessOperation, RecoveryClass};

pub(crate) struct SegmentedSpool {
    directory: PathBuf,
    stream: OutputStream,
    file: File,
    segment_bytes: u64,
    segment_index: u64,
    segment_written: u64,
}

pub(crate) struct SpoolWriteFailure {
    written: u64,
    error: ProcessError,
}

impl SpoolWriteFailure {
    pub(crate) const fn written(&self) -> u64 {
        self.written
    }

    pub(crate) fn into_error(self) -> ProcessError {
        self.error
    }
}

impl SegmentedSpool {
    fn create(
        directory: &Path,
        stream: OutputStream,
        segment_bytes: u64,
    ) -> Result<Self, ProcessError> {
        fs::create_dir_all(directory)
            .map_err(|error| spool_io_error("spool directory cannot be created", error))?;
        let file = create_segment(directory, stream, 0)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            stream,
            file,
            segment_bytes,
            segment_index: 0,
            segment_written: 0,
        })
    }

    pub(crate) fn write(&mut self, mut bytes: &[u8]) -> Result<u64, SpoolWriteFailure> {
        let mut written = 0_u64;
        while !bytes.is_empty() {
            if self.segment_written == self.segment_bytes {
                self.rotate().map_err(|error| SpoolWriteFailure { written, error })?;
            }
            let available = self
                .segment_bytes
                .checked_sub(self.segment_written)
                .ok_or_else(|| SpoolWriteFailure {
                    written,
                    error: spool_error("spool segment accounting is inconsistent"),
                })?;
            let remaining = u64::try_from(bytes.len())
                .map_err(|_| SpoolWriteFailure {
                    written,
                    error: spool_error("spool chunk is unrepresentable"),
                })?;
            let write_length = usize::try_from(available.min(remaining))
                .map_err(|_| SpoolWriteFailure {
                    written,
                    error: spool_error("spool write length is unrepresentable"),
                })?;
            let count_usize = loop {
                match self.file.write(&bytes[..write_length]) {
                    Ok(0) => {
                        let source = std::io::Error::new(
                            ErrorKind::WriteZero,
                            "stream spool made no write progress",
                        );
                        return Err(SpoolWriteFailure {
                            written,
                            error: spool_io_error("stream spool write failed", source),
                        });
                    }
                    Ok(count) => break count,
                    Err(error) if error.kind() == ErrorKind::Interrupted => {}
                    Err(error) => {
                        return Err(SpoolWriteFailure {
                            written,
                            error: spool_io_error("stream spool write failed", error),
                        });
                    }
                }
            };
            let count = u64::try_from(count_usize).map_err(|_| SpoolWriteFailure {
                written,
                error: spool_error("spool write length is unrepresentable"),
            })?;
            self.segment_written = self
                .segment_written
                .checked_add(count)
                .ok_or_else(|| SpoolWriteFailure {
                    written,
                    error: spool_error("spool segment accounting overflowed"),
                })?;
            written = written.checked_add(count).ok_or_else(|| SpoolWriteFailure {
                written,
                error: spool_error("spool output accounting overflowed"),
            })?;
            bytes = &bytes[count_usize..];
        }
        Ok(written)
    }

    fn rotate(&mut self) -> Result<(), ProcessError> {
        synchronize_file(&mut self.file)?;
        let next = self
            .segment_index
            .checked_add(1)
            .ok_or_else(|| spool_error("spool segment index overflowed"))?;
        self.file = create_segment(&self.directory, self.stream, next)?;
        self.segment_index = next;
        self.segment_written = 0;
        Ok(())
    }

    pub(crate) fn synchronize(&mut self) -> Result<(), ProcessError> {
        synchronize_file(&mut self.file)
    }
}

fn create_segment(
    directory: &Path,
    stream: OutputStream,
    index: u64,
) -> Result<File, ProcessError> {
    let path = directory.join(format!("{}.{index:020}", stream_spool_name(stream)));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            spool_io_error("exclusive stream spool segment cannot be created", error)
        })
}

fn synchronize_file(file: &mut File) -> Result<(), ProcessError> {
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|error| spool_io_error("stream spool cannot be synchronized", error))
}

pub(crate) const fn stream_spool_name(stream: OutputStream) -> &'static str {
    match stream {
        OutputStream::Stdout => "stdout.spool",
        OutputStream::Stderr => "stderr.spool",
        OutputStream::Terminal => "terminal.spool",
    }
}

pub(crate) struct SpoolSet {
    pub(crate) stdout: Option<SegmentedSpool>,
    pub(crate) stderr: Option<SegmentedSpool>,
    pub(crate) terminal: Option<SegmentedSpool>,
}

impl SpoolSet {
    pub(crate) fn pipes(directory: &Path, segment_bytes: u64) -> Result<Self, ProcessError> {
        Ok(Self {
            stdout: Some(SegmentedSpool::create(
                directory,
                OutputStream::Stdout,
                segment_bytes,
            )?),
            stderr: Some(SegmentedSpool::create(
                directory,
                OutputStream::Stderr,
                segment_bytes,
            )?),
            terminal: None,
        })
    }

    pub(crate) fn pty(directory: &Path, segment_bytes: u64) -> Result<Self, ProcessError> {
        Ok(Self {
            stdout: None,
            stderr: None,
            terminal: Some(SegmentedSpool::create(
                directory,
                OutputStream::Terminal,
                segment_bytes,
            )?),
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

fn spool_io_error(detail: &'static str, source: std::io::Error) -> ProcessError {
    ProcessError::with_source(
        ErrorCode::Output,
        ProcessOperation::Stream,
        RecoveryClass::ReopenAndReconcile,
        detail,
        source,
    )
}
