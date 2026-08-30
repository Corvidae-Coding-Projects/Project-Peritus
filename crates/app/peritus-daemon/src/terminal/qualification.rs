//! Bounded operational qualification of the host's real combined PTY stream.

mod reader;

use core::fmt;
use std::error::Error;

use portable_pty::{Child, CommandBuilder, NativePtySystem, PtySize, PtySystem};

use self::reader::{QualifiedEvent, next_sequence, push_event, read_terminal, summarize};

const BUFFER_LIMIT_BYTES: usize = 8;
const BUFFER_LIMIT_BYTES_U64: u64 = 8;
const MAXIMUM_EVENTS: usize = 256;

/// Direct facts observed while exercising one real host pseudo-terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PtyQualificationObservation {
    output_bytes: u64,
    sequence_strictly_increasing: bool,
    offsets_conserved: bool,
    combined_stream_only: bool,
    exit_records: u64,
    peak_buffered_bytes: u64,
    configured_buffer_limit: u64,
}

impl PtyQualificationObservation {
    /// Returns bytes read from the real combined PTY stream.
    #[must_use]
    pub(crate) const fn output_bytes(self) -> u64 {
        self.output_bytes
    }

    /// Returns whether every observed event sequence increased strictly.
    #[must_use]
    pub(crate) const fn sequence_strictly_increasing(self) -> bool {
        self.sequence_strictly_increasing
    }

    /// Returns whether output and exit offsets conserved every observed byte.
    #[must_use]
    pub(crate) const fn offsets_conserved(self) -> bool {
        self.offsets_conserved
    }

    /// Returns whether every output observation came from the combined terminal stream.
    #[must_use]
    pub(crate) const fn combined_stream_only(self) -> bool {
        self.combined_stream_only
    }

    /// Returns the number of real child exits recorded by the qualifier.
    #[must_use]
    pub(crate) const fn exit_records(self) -> u64 {
        self.exit_records
    }

    /// Returns the largest number of terminal bytes retained during one read.
    #[must_use]
    pub(crate) const fn peak_buffered_bytes(self) -> u64 {
        self.peak_buffered_bytes
    }

    /// Returns the positive byte ceiling enforced for each terminal read.
    #[must_use]
    pub(crate) const fn configured_buffer_limit(self) -> u64 {
        self.configured_buffer_limit
    }
}

/// Failure to allocate, launch, observe, or synchronously reap the qualification PTY.
#[derive(Debug)]
pub struct PtyQualificationError {
    kind: PtyQualificationErrorKind,
    operation: &'static str,
    detail: &'static str,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl fmt::Display for PtyQualificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}: {}", self.kind, self.operation, self.detail)?;
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl Error for PtyQualificationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|source| &**source as &(dyn Error + 'static))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PtyQualificationErrorKind {
    BufferAllocation,
    PtyAllocation,
    ReaderCreation,
    ReaderThread,
    Spawn,
    Read,
    EventLimit,
    Arithmetic,
    Wait,
    ChildFailed,
}

/// Launches a fixed real PTY child and derives ordering facts from its observed bytes and exit.
///
/// The child writes a fixed payload to both standard output and standard error. A bounded reader
/// drains that combined stream concurrently so neither a platform buffer nor child reaping can
/// wait on the other. The qualifier then reaps the child, closes the master owner to make Windows
/// `ConPTY` publish EOF, and joins the reader before accepting any observation. Reads use a fixed
/// positive byte buffer.
///
/// # Errors
///
/// Returns a typed failure when PTY allocation, child launch, bounded stream observation, or
/// synchronous child reaping fails. A non-successful child exit also fails qualification.
pub fn qualify_pty_ordering() -> Result<PtyQualificationObservation, PtyQualificationError> {
    let mut events = Vec::new();
    events.try_reserve_exact(MAXIMUM_EVENTS).map_err(|source| {
        PtyQualificationError::with_source(
            PtyQualificationErrorKind::BufferAllocation,
            "reserve observation metadata",
            "the bounded event buffer could not be allocated",
            Box::new(source),
        )
    })?;
    let mut read_buffer = Vec::new();
    read_buffer.try_reserve_exact(BUFFER_LIMIT_BYTES).map_err(|source| {
        PtyQualificationError::with_source(
            PtyQualificationErrorKind::BufferAllocation,
            "reserve terminal read buffer",
            "the positive terminal byte buffer could not be allocated",
            Box::new(source),
        )
    })?;
    read_buffer.resize(BUFFER_LIMIT_BYTES, 0);

    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(PtySize::default()).map_err(|source| {
        PtyQualificationError::with_source(
            PtyQualificationErrorKind::PtyAllocation,
            "allocate host pseudo-terminal",
            "the host rejected PTY allocation",
            source.into_boxed_dyn_error(),
        )
    })?;
    let reader = pair.master.try_clone_reader().map_err(|source| {
        PtyQualificationError::with_source(
            PtyQualificationErrorKind::ReaderCreation,
            "open combined terminal reader",
            "the PTY master could not produce a reader",
            source.into_boxed_dyn_error(),
        )
    })?;
    let child = pair.slave.spawn_command(qualification_command()).map_err(|source| {
        PtyQualificationError::with_source(
            PtyQualificationErrorKind::Spawn,
            "spawn PTY qualification child",
            "the fixed qualification command could not be started",
            source.into_boxed_dyn_error(),
        )
    })?;
    drop(pair.slave);
    let mut child = PtyChildOwner::new(child);
    let reader_thread = std::thread::Builder::new()
        .name("peritus-pty-qualification-reader".to_owned())
        .spawn(move || read_terminal(reader, events, read_buffer))
        .map_err(|source| {
            PtyQualificationError::with_source(
                PtyQualificationErrorKind::ReaderThread,
                "start combined terminal reader",
                "the bounded PTY reader thread could not be started",
                Box::new(source),
            )
        })?;
    let status = child.wait();
    drop(child);
    drop(pair.master);
    let read_result = reader_thread.join();
    let status = status?;
    let (mut events, output_offset, peak_buffered_bytes) = read_result.map_err(|_| {
        PtyQualificationError::rejected(
            PtyQualificationErrorKind::ReaderThread,
            "join combined terminal reader",
            "the bounded PTY reader thread panicked",
        )
    })??;

    let exit_sequence = next_sequence(&events)?;
    push_event(
        &mut events,
        QualifiedEvent::Exited { sequence: exit_sequence, offset: output_offset },
    )?;
    if !status.success() {
        return Err(PtyQualificationError::rejected(
            PtyQualificationErrorKind::ChildFailed,
            "observe PTY qualification child",
            "the fixed qualification command returned an unsuccessful exit",
        ));
    }

    Ok(summarize(&events, peak_buffered_bytes))
}

impl PtyQualificationError {
    const fn rejected(
        kind: PtyQualificationErrorKind,
        operation: &'static str,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, detail, source: None }
    }

    fn with_source(
        kind: PtyQualificationErrorKind,
        operation: &'static str,
        detail: &'static str,
        source: Box<dyn Error + Send + Sync + 'static>,
    ) -> Self {
        Self { kind, operation, detail, source: Some(source) }
    }
}

struct PtyChildOwner {
    child: Option<Box<dyn Child + Send + Sync>>,
}

impl PtyChildOwner {
    const fn new(child: Box<dyn Child + Send + Sync>) -> Self {
        Self { child: Some(child) }
    }

    fn wait(&mut self) -> Result<portable_pty::ExitStatus, PtyQualificationError> {
        let result = self.child.as_mut().map_or_else(
            || {
                Err(PtyQualificationError::rejected(
                    PtyQualificationErrorKind::Wait,
                    "wait for PTY qualification child",
                    "the child owner no longer holds a live wait handle",
                ))
            },
            |child| {
                child.wait().map_err(|source| {
                    PtyQualificationError::with_source(
                        PtyQualificationErrorKind::Wait,
                        "wait for PTY qualification child",
                        "the child wait operation failed",
                        Box::new(source),
                    )
                })
            },
        )?;
        self.child = None;
        Ok(result)
    }
}

impl Drop for PtyChildOwner {
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else { return };
        if let Ok(Some(_)) = child.try_wait() {
        } else {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
fn qualification_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("/bin/sh");
    command.env_clear();
    command.args([
        "-c",
        "printf 'peritus-out-1\\n'; printf 'peritus-err-2\\n' >&2; printf 'peritus-out-3\\n'",
    ]);
    command
}

#[cfg(windows)]
fn qualification_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("cmd.exe");
    command.args([
        "/D",
        "/S",
        "/C",
        "echo peritus-out-1&echo peritus-err-2 1>&2&echo peritus-out-3",
    ]);
    command
}
