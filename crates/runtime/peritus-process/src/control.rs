//! Bounded non-owning process control and observation handle.

use std::{
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{SyncSender, TrySendError},
    },
    time::Duration,
};

mod cancellation;
mod input;

pub(crate) use cancellation::{CancellationOwner, lane as cancellation_lane};
pub(crate) use input::{InputCommand, InputLane, InputOwner, lane as input_lane};

use crate::{
    CancellationReason, ControlRejection, ErrorCode, ProcessCursor, ProcessError, ProcessEvent,
    ProcessOperation,
    NativeSessionRecovery, ProcessTreeIdentity, RecoveryClass, TerminalCapabilities,
    TerminalResult, TerminalSize,
    events::EventLog,
    retained_owner::{RetainedOwnerObservation, RetainedProcessKey, RetainedProcessTransport},
};

pub(crate) enum ControlCommand {
    Resize(TerminalSize),
    Signal(ProcessSignal),
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
    pub(crate) tree: Option<ProcessTreeIdentity>,
    pub(crate) native_recovery: Option<NativeSessionRecovery>,
    pub(crate) terminal: Option<TerminalResult>,
}

pub(crate) struct SharedObservation {
    pub(crate) state: Mutex<SharedExecution>,
    pub(crate) changed: Condvar,
}

/// Cloneable bounded command sender and read-only execution observer.
#[derive(Clone)]
pub struct ProcessControl {
    owner: ControlOwner,
    terminal: TerminalCapabilities,
}

#[derive(Clone)]
enum ControlOwner {
    Local {
        sender: SyncSender<ControlCommand>,
        cancellation: cancellation::CancellationLane,
        input: InputLane,
        shared: Arc<SharedObservation>,
    },
    Retained {
        transport: Arc<dyn RetainedProcessTransport>,
        key: RetainedProcessKey,
        latest: Arc<Mutex<Option<RetainedOwnerObservation>>>,
        streams: Arc<Mutex<[Option<Vec<u8>>; 3]>>,
    },
}

const RETAINED_STREAM_PAGE_BYTES: usize = 64 * 1_024;

impl ProcessControl {
    pub(crate) const fn new(
        sender: SyncSender<ControlCommand>,
        cancellation: cancellation::CancellationLane,
        input: InputLane,
        shared: Arc<SharedObservation>,
        terminal: TerminalCapabilities,
    ) -> Self {
        Self {
            owner: ControlOwner::Local { sender, cancellation, input, shared },
            terminal,
        }
    }

    pub(crate) fn new_retained(
        transport: Arc<dyn RetainedProcessTransport>,
        key: RetainedProcessKey,
        terminal: TerminalCapabilities,
    ) -> Self {
        Self {
            owner: ControlOwner::Retained {
                transport,
                key,
                latest: Arc::new(Mutex::new(None)),
                streams: Arc::new(Mutex::new(std::array::from_fn(|_| None))),
            },
            terminal,
        }
    }

    /// Queues one bounded literal stdin write without blocking on a full control queue.
    ///
    /// # Errors
    ///
    /// Returns a typed error when input is closed, the write exceeds its per-write bound, the
    /// queue is full, or the process owner has terminated.
    pub fn write_stdin(&self, bytes: Vec<u8>) -> Result<(), ProcessError> {
        match &self.owner {
            ControlOwner::Local { input, .. } => input.write(bytes),
            ControlOwner::Retained { transport, key, .. } => {
                transport.write_stdin(*key, bytes, false)
            }
        }
    }

    /// Queues one bounded literal stdin write, waiting for bounded queue capacity.
    ///
    /// # Errors
    ///
    /// Returns a typed error when input is closed, the write exceeds its per-write bound, or the
    /// process owner has terminated.
    pub fn write_stdin_blocking(&self, bytes: Vec<u8>) -> Result<(), ProcessError> {
        match &self.owner {
            ControlOwner::Local { input, .. } => input.write_blocking(bytes),
            ControlOwner::Retained { transport, key, .. } => {
                transport.write_stdin(*key, bytes, true)
            }
        }
    }

    /// Queues one bounded literal stdin write while the caller permits waiting.
    ///
    /// `Ok(false)` means the caller stopped waiting before the command entered the queue. No
    /// write is accepted in that case. Queue pressure has no elapsed deadline.
    ///
    /// # Errors
    /// Returns a typed error when input is invalid or the process owner has terminated.
    pub fn write_stdin_while(
        &self,
        bytes: Vec<u8>,
        keep_waiting: impl FnMut() -> bool,
    ) -> Result<bool, ProcessError> {
        match &self.owner {
            ControlOwner::Local { input, .. } => input.write_while(bytes, keep_waiting),
            ControlOwner::Retained { transport, key, .. } => {
                retained_write_while(transport, *key, bytes, keep_waiting)
            }
        }
    }

    /// Queues idempotent input closure.
    ///
    /// # Errors
    ///
    /// Returns an error when the bounded input queue is full or closed.
    pub fn close_stdin(&self) -> Result<(), ProcessError> {
        match &self.owner {
            ControlOwner::Local { input, .. } => input.close(),
            ControlOwner::Retained { transport, key, .. } => {
                transport.close_stdin(*key, false)
            }
        }
    }

    /// Queues idempotent input closure, waiting for bounded queue capacity.
    ///
    /// # Errors
    ///
    /// Returns an error when the process owner has terminated.
    pub fn close_stdin_blocking(&self) -> Result<(), ProcessError> {
        match &self.owner {
            ControlOwner::Local { input, .. } => input.close_blocking(),
            ControlOwner::Retained { transport, key, .. } => {
                transport.close_stdin(*key, true)
            }
        }
    }

    /// Queues idempotent input closure while the caller permits waiting.
    ///
    /// `Ok(false)` means the caller stopped waiting before closure entered the queue.
    ///
    /// # Errors
    /// Returns an error when the process owner has terminated.
    pub fn close_stdin_while(
        &self,
        mut keep_waiting: impl FnMut() -> bool,
    ) -> Result<bool, ProcessError> {
        match &self.owner {
            ControlOwner::Local { input, .. } => input.close_while(keep_waiting),
            ControlOwner::Retained { transport, key, .. } => {
                loop {
                    if !keep_waiting() {
                        return Ok(false);
                    }
                    match transport.close_stdin(*key, false) {
                        Err(error)
                            if error.control_rejection()
                                == Some(ControlRejection::Backpressure) =>
                        {
                            std::thread::yield_now();
                        }
                        result => return result.map(|()| true),
                    }
                }
            }
        }
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
            )
            .rejecting_control(ControlRejection::InvalidRequest));
        }
        match &self.owner {
            ControlOwner::Local { sender, .. } => try_send(sender, ControlCommand::Resize(size)),
            ControlOwner::Retained { transport, key, .. } => transport.resize(*key, size),
        }
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
            )
            .rejecting_control(ControlRejection::InvalidRequest));
        }
        match &self.owner {
            ControlOwner::Local { sender, .. } => try_send(sender, ControlCommand::Signal(signal)),
            ControlOwner::Retained { transport, key, .. } => transport.signal(*key, signal),
        }
    }

    /// Queues an idempotent stop request.
    ///
    /// # Errors
    ///
    /// Returns an error when the process owner has already closed its lifecycle.
    pub fn cancel(&self, reason: CancellationReason) -> Result<(), ProcessError> {
        match &self.owner {
            ControlOwner::Local { cancellation, .. } => cancellation.request(reason),
            ControlOwner::Retained { transport, key, .. } => transport.cancel(*key, reason),
        }
    }

    /// Queues an idempotent stop request if the caller still permits admission.
    ///
    /// `Ok(false)` means the caller stopped waiting before the capacity-free cancellation slot
    /// was updated.
    ///
    /// # Errors
    /// Returns an error when the process owner has terminated before accepting the request.
    pub fn cancel_while(
        &self,
        reason: CancellationReason,
        mut keep_waiting: impl FnMut() -> bool,
    ) -> Result<bool, ProcessError> {
        match &self.owner {
            ControlOwner::Local { cancellation, .. } => {
                cancellation.request_while(reason, keep_waiting)
            }
            ControlOwner::Retained { transport, key, .. } => {
                if !keep_waiting() {
                    Ok(false)
                } else {
                    transport.cancel(*key, reason).map(|()| true)
                }
            }
        }
    }

    /// Reads at most `max_events` retained events after a cursor.
    #[must_use]
    pub fn read_events(&self, cursor: ProcessCursor, max_events: usize) -> Vec<ProcessEvent> {
        match &self.owner {
            ControlOwner::Local { shared, .. } => {
                let state =
                    shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let events = state.events.read(cursor, max_events);
                drop(state);
                events
            }
            ControlOwner::Retained { transport, key, latest, .. } => {
                refresh_retained(transport, *key, latest, cursor, max_events, None)
                    .map_or_else(|_| Vec::new(), |observation| observation.events().to_vec())
            }
        }
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
        match &self.owner {
            ControlOwner::Local { shared, .. } => {
                let state =
                    shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let (state, _) = shared
                    .changed
                    .wait_timeout_while(state, timeout, |current| {
                        current.terminal.is_none() && current.events.read(cursor, 1).is_empty()
                    })
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let events = state.events.read(cursor, max_events);
                drop(state);
                events
            }
            ControlOwner::Retained { transport, key, latest, .. } => {
                refresh_retained(transport, *key, latest, cursor, max_events, Some(timeout))
                    .map_or_else(|_| Vec::new(), |observation| observation.events().to_vec())
            }
        }
    }

    /// Returns the current bounded tail in canonical stdout, stderr, terminal order.
    ///
    /// Stream bytes remain contiguous even when operating-system readers observe chunks in an
    /// interleaved order. A newline separates adjacent nonempty streams.
    #[must_use]
    pub fn retained_output(&self) -> Vec<u8> {
        let streams = [
            self.retained_stream_output(crate::OutputStream::Stdout),
            self.retained_stream_output(crate::OutputStream::Stderr),
            self.retained_stream_output(crate::OutputStream::Terminal),
        ];
        let mut output = Vec::with_capacity(
            streams.iter().map(Vec::len).fold(2_usize, usize::saturating_add),
        );
        for stream in streams {
            append_stream(&mut output, &stream);
        }
        output
    }

    /// Returns the current bounded tail for one exact stream.
    #[must_use]
    pub fn retained_stream_output(&self, stream: crate::OutputStream) -> Vec<u8> {
        match &self.owner {
            ControlOwner::Local { shared, .. } => {
                let state =
                    shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                match stream {
                    crate::OutputStream::Stdout => state.retained_stdout.clone(),
                    crate::OutputStream::Stderr => state.retained_stderr.clone(),
                    crate::OutputStream::Terminal => state.retained_terminal.clone(),
                }
            }
            ControlOwner::Retained { transport, key, streams, .. } => {
                match read_retained_stream(transport, *key, stream) {
                    Ok(output) => {
                        streams
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            [stream_index(stream)] = Some(output.clone());
                        output
                    }
                    Err(_) => streams
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        [stream_index(stream)]
                        .clone()
                        .unwrap_or_default(),
                }
            }
        }
    }

    /// Returns the terminal result after publication.
    #[must_use]
    pub fn terminal_result(&self) -> Option<TerminalResult> {
        match &self.owner {
            ControlOwner::Local { shared, .. } => shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .terminal
                .clone(),
            ControlOwner::Retained { transport, key, latest, .. } => retained_snapshot(
                transport,
                *key,
                latest,
            )
            .and_then(|value| value.terminal_result().cloned()),
        }
    }

    /// Returns whether the unique supervisor owner has actually finished.
    ///
    /// This remains false while terminal persistence or publication is still running, even after
    /// cancellation admission has closed or the operating-system process has exited.
    #[must_use]
    pub fn owner_finished(&self) -> bool {
        self.try_owner_finished().unwrap_or(false)
    }

    /// Returns whether the unique owner finished, preserving retained-transport failures.
    ///
    /// # Errors
    /// Returns a typed error when the retained service-owner generation cannot be observed.
    pub(crate) fn try_owner_finished(&self) -> Result<bool, ProcessError> {
        match &self.owner {
            ControlOwner::Local { cancellation, .. } => Ok(cancellation.owner_finished()),
            ControlOwner::Retained { transport, key, latest, .. } => {
                refresh_retained(
                    transport,
                    *key,
                    latest,
                    ProcessCursor::after(0),
                    0,
                    None,
                )
                .map(|value| value.owner_finished())
            }
        }
    }

    /// Returns the exact operating-system process-tree identity once native launch succeeds.
    ///
    /// `None` means the owner has not yet published birth identity or launch failed before a live
    /// process existed. Callers should observe the process event stream before retrying.
    #[must_use]
    pub fn tree_identity(&self) -> Option<ProcessTreeIdentity> {
        match &self.owner {
            ControlOwner::Local { shared, .. } => shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .tree,
            ControlOwner::Retained { transport, key, latest, .. } => {
                retained_snapshot(transport, *key, latest).and_then(|value| value.tree_identity())
            }
        }
    }

    /// Returns exact platform recovery evidence from the current process owner.
    ///
    /// `None` keeps unavailable native custody distinct from clean release.
    #[must_use]
    pub fn native_recovery(&self) -> Option<NativeSessionRecovery> {
        match &self.owner {
            ControlOwner::Local { shared, .. } => shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .native_recovery
                .clone(),
            ControlOwner::Retained { transport, key, latest, .. } => retained_snapshot(
                transport,
                *key,
                latest,
            )
            .and_then(|value| value.native_recovery().cloned()),
        }
    }
}

fn try_send(
    sender: &SyncSender<ControlCommand>,
    command: ControlCommand,
) -> Result<(), ProcessError> {
    sender.try_send(command).map_err(|error| match error {
            TrySendError::Full(_) => ProcessError::new(
                ErrorCode::Input,
                ProcessOperation::Control,
                RecoveryClass::CorrectRequest,
                "bounded process control queue is full",
            )
            .rejecting_control(ControlRejection::Backpressure),
            TrySendError::Disconnected(_) => ProcessError::new(
                ErrorCode::Input,
                ProcessOperation::Control,
                RecoveryClass::Terminal,
                "process owner has already terminated",
            )
            .rejecting_control(ControlRejection::AdmissionClosed),
        })
}

fn retained_write_while(
    transport: &Arc<dyn RetainedProcessTransport>,
    key: RetainedProcessKey,
    bytes: Vec<u8>,
    mut keep_waiting: impl FnMut() -> bool,
) -> Result<bool, ProcessError> {
    loop {
        if !keep_waiting() {
            return Ok(false);
        }
        match transport.write_stdin(key, bytes.clone(), false) {
            Err(error)
                if error.control_rejection() == Some(ControlRejection::Backpressure) =>
            {
                std::thread::yield_now();
            }
            result => return result.map(|()| true),
        }
    }
}

fn retained_snapshot(
    transport: &Arc<dyn RetainedProcessTransport>,
    key: RetainedProcessKey,
    latest: &Arc<Mutex<Option<RetainedOwnerObservation>>>,
) -> Option<RetainedOwnerObservation> {
    refresh_retained(transport, key, latest, ProcessCursor::after(0), 0, None)
        .ok()
        .or_else(|| {
            latest.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
        })
}

fn refresh_retained(
    transport: &Arc<dyn RetainedProcessTransport>,
    key: RetainedProcessKey,
    latest: &Arc<Mutex<Option<RetainedOwnerObservation>>>,
    cursor: ProcessCursor,
    max_events: usize,
    wait: Option<Duration>,
) -> Result<RetainedOwnerObservation, ProcessError> {
    let observation = transport.observe(key, cursor, max_events, wait)?;
    *latest.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
        Some(observation.clone());
    Ok(observation)
}

fn read_retained_stream(
    transport: &Arc<dyn RetainedProcessTransport>,
    key: RetainedProcessKey,
    stream: crate::OutputStream,
) -> Result<Vec<u8>, ProcessError> {
    let mut digest = None;
    let mut total = None;
    let mut offset = 0_u64;
    let mut output = Vec::new();
    loop {
        let page = transport.read_stream_page(
            key,
            stream,
            digest,
            offset,
            RETAINED_STREAM_PAGE_BYTES,
        )?;
        if page.stream() != stream
            || page.offset() != offset
            || digest.is_some_and(|value| value != page.snapshot_digest())
            || total.is_some_and(|value| value != page.total_bytes())
        {
            return Err(invalid_retained_stream());
        }
        digest = Some(page.snapshot_digest());
        total = Some(page.total_bytes());
        output
            .try_reserve(page.bytes().len())
            .map_err(|_| invalid_retained_stream())?;
        output.extend_from_slice(page.bytes());
        offset = page.next_offset();
        if page.complete() {
            if u64::try_from(output.len()).ok() != total
                || peritus_codec::sha256(&output) != page.snapshot_digest()
            {
                return Err(invalid_retained_stream());
            }
            return Ok(output);
        }
    }
}

const fn stream_index(stream: crate::OutputStream) -> usize {
    match stream {
        crate::OutputStream::Stdout => 0,
        crate::OutputStream::Stderr => 1,
        crate::OutputStream::Terminal => 2,
    }
}

const fn invalid_retained_stream() -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        "retained stream snapshot is inconsistent",
    )
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
