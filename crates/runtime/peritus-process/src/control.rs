//! Bounded non-owning process control and observation handle.

use std::{
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{SyncSender, TrySendError},
    },
    time::Duration,
};

use crate::{
    CancellationReason, ErrorCode, ProcessCursor, ProcessError, ProcessEvent, ProcessOperation,
    RecoveryClass, StdinPolicy, TerminalCapabilities, TerminalResult, TerminalSize,
    events::EventLog,
};

pub(crate) enum ControlCommand {
    Write(Vec<u8>),
    CloseInput,
    Resize(TerminalSize),
    Signal(ProcessSignal),
    Cancel(CancellationReason),
}

/// Closed portable signal vocabulary supported by owned native process trees.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProcessSignal {
    /// Request an interactive interrupt (`SIGINT` or the platform-native equivalent).
    Interrupt,
    /// Request graceful termination (`SIGTERM` or the platform-native equivalent).
    Terminate,
}

pub(crate) struct SharedExecution {
    pub(crate) events: EventLog,
    pub(crate) retained_stdout: Vec<u8>,
    pub(crate) retained_stderr: Vec<u8>,
    pub(crate) retained_terminal: Vec<u8>,
    pub(crate) terminal: Option<TerminalResult>,
}

pub(crate) struct SharedObservation {
    pub(crate) state: Mutex<SharedExecution>,
    pub(crate) changed: Condvar,
}

/// Cloneable bounded command sender and read-only execution observer.
#[derive(Clone)]
pub struct ProcessControl {
    sender: SyncSender<ControlCommand>,
    shared: Arc<SharedObservation>,
    stdin_policy: StdinPolicy,
    terminal: TerminalCapabilities,
}

impl ProcessControl {
    pub(crate) const fn new(
        sender: SyncSender<ControlCommand>,
        shared: Arc<SharedObservation>,
        stdin_policy: StdinPolicy,
        terminal: TerminalCapabilities,
    ) -> Self {
        Self { sender, shared, stdin_policy, terminal }
    }

    /// Queues one bounded literal stdin write without blocking on a full control queue.
    ///
    /// # Errors
    ///
    /// Returns a typed error when input is closed, the write exceeds its per-write bound, the
    /// queue is full, or the process owner has terminated.
    pub fn write_stdin(&self, bytes: Vec<u8>) -> Result<(), ProcessError> {
        match self.stdin_policy {
            StdinPolicy::Closed => return Err(input_error("stdin is disabled for this process")),
            StdinPolicy::Bounded { max_write_bytes, .. } => {
                if bytes.is_empty()
                    || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_write_bytes
                {
                    return Err(input_error("stdin write is empty or exceeds its per-write bound"));
                }
            }
        }
        self.try_send(ControlCommand::Write(bytes))
    }

    /// Queues idempotent input closure.
    ///
    /// # Errors
    ///
    /// Returns an error when the bounded control queue is full or closed.
    pub fn close_stdin(&self) -> Result<(), ProcessError> {
        self.try_send(ControlCommand::CloseInput)
    }

    /// Queues a checked PTY resize.
    ///
    /// # Errors
    ///
    /// Returns an error when the bounded control queue is full or closed. Pipe-mode owners reject
    /// the resize with a process event and terminally preserve normal execution.
    pub fn resize(&self, size: TerminalSize) -> Result<(), ProcessError> {
        if !self.terminal.resize_allowed() {
            return Err(ProcessError::new(
                ErrorCode::InvalidInput,
                ProcessOperation::Control,
                RecoveryClass::CorrectRequest,
                "terminal resize was not authorized by the checked execution plan",
            ));
        }
        self.try_send(ControlCommand::Resize(size))
    }

    /// Queues one portable signal without classifying the operation as cancellation.
    ///
    /// # Errors
    /// Returns an error when terminal signal delivery was not authorized, or when the bounded
    /// control queue is full or closed.
    pub fn signal(&self, signal: ProcessSignal) -> Result<(), ProcessError> {
        if !self.terminal.signals_allowed() {
            return Err(ProcessError::new(
                ErrorCode::InvalidInput,
                ProcessOperation::Control,
                RecoveryClass::CorrectRequest,
                "process signals were not authorized by the checked execution plan",
            ));
        }
        self.try_send(ControlCommand::Signal(signal))
    }

    /// Queues an idempotent stop request.
    ///
    /// # Errors
    ///
    /// Returns an error when the bounded control queue is full or closed.
    pub fn cancel(&self, reason: CancellationReason) -> Result<(), ProcessError> {
        self.try_send(ControlCommand::Cancel(reason))
    }

    /// Reads at most `max_events` retained events after a cursor.
    #[must_use]
    pub fn read_events(&self, cursor: ProcessCursor, max_events: usize) -> Vec<ProcessEvent> {
        let state = self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let events = state.events.read(cursor, max_events);
        drop(state);
        events
    }

    /// Waits for a newer event or terminal result, then returns a bounded page.
    #[must_use]
    #[allow(
        clippy::significant_drop_tightening,
        reason = "the mutex guard must remain held while entering the condition-variable wait"
    )]
    pub fn wait_events(
        &self,
        cursor: ProcessCursor,
        max_events: usize,
        timeout: Duration,
    ) -> Vec<ProcessEvent> {
        let state = self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (state, _) = self
            .shared
            .changed
            .wait_timeout_while(state, timeout, |current| {
                current.terminal.is_none() && current.events.read(cursor, 1).is_empty()
            })
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let events = state.events.read(cursor, max_events);
        drop(state);
        events
    }

    /// Returns the current bounded tail in canonical stdout, stderr, terminal order.
    ///
    /// Stream bytes remain contiguous even when operating-system readers observe chunks in an
    /// interleaved order. A newline separates adjacent nonempty streams.
    #[must_use]
    pub fn retained_output(&self) -> Vec<u8> {
        let state = self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut output = Vec::with_capacity(
            state
                .retained_stdout
                .len()
                .saturating_add(state.retained_stderr.len())
                .saturating_add(state.retained_terminal.len())
                .saturating_add(2),
        );
        append_stream(&mut output, &state.retained_stdout);
        append_stream(&mut output, &state.retained_stderr);
        append_stream(&mut output, &state.retained_terminal);
        output
    }

    /// Returns the current bounded tail for one exact stream.
    #[must_use]
    pub fn retained_stream_output(&self, stream: crate::OutputStream) -> Vec<u8> {
        let state = self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match stream {
            crate::OutputStream::Stdout => state.retained_stdout.clone(),
            crate::OutputStream::Stderr => state.retained_stderr.clone(),
            crate::OutputStream::Terminal => state.retained_terminal.clone(),
        }
    }

    /// Returns the terminal result after publication.
    #[must_use]
    pub fn terminal_result(&self) -> Option<TerminalResult> {
        self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).terminal.clone()
    }

    fn try_send(&self, command: ControlCommand) -> Result<(), ProcessError> {
        self.sender.try_send(command).map_err(|error| match error {
            TrySendError::Full(_) => ProcessError::new(
                ErrorCode::Input,
                ProcessOperation::Control,
                RecoveryClass::CorrectRequest,
                "bounded process control queue is full",
            ),
            TrySendError::Disconnected(_) => ProcessError::new(
                ErrorCode::Input,
                ProcessOperation::Control,
                RecoveryClass::Terminal,
                "process owner has already terminated",
            ),
        })
    }
}

fn append_stream(output: &mut Vec<u8>, stream: &[u8]) {
    if stream.is_empty() {
        return;
    }
    if !output.is_empty() && !output.ends_with(b"\n") {
        output.push(b'\n');
    }
    output.extend_from_slice(stream);
}

const fn input_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Input,
        ProcessOperation::Control,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
