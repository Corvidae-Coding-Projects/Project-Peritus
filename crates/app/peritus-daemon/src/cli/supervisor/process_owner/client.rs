//! Daemon-attempt client for the service supervisor's private retained-owner pipes.

use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::Mutex,
    time::{Duration, Instant},
};

use peritus_process::{
    CancellationReason, ControlRejection, ErrorCode, ProcessCursor, ProcessError,
    OutputStream, ProcessOperation, ProcessSignal, RecoveryClass, RetainedOwnerObservation,
    RetainedProcessKey, RetainedProcessTransport, RetainedServiceOwner, RetainedStreamPage,
    TerminalResult, TerminalSize,
};
use peritus_types::Sha256Digest;

use super::protocol::{self, Command, Reply};

struct Connection {
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

#[derive(Clone)]
struct Attachment {
    request: Vec<u8>,
    digest: Sha256Digest,
    attached: bool,
}

/// One daemon generation's serialized client for inherited supervisor pipes.
pub(super) struct OwnerClient {
    service_owner: RetainedServiceOwner,
    connection: Mutex<Option<Connection>>,
    attachments: Mutex<HashMap<RetainedProcessKey, Attachment>>,
}

const OWNER_POLL_INTERVAL: Duration = Duration::from_millis(5);
const OWNER_EVENT_PAGE: usize = 256;
const OWNER_STREAM_PAGE_BYTES: usize = 64 * 1_024;

impl OwnerClient {
    pub(super) fn inherited(owner_token: &[u8]) -> Self {
        Self {
            service_owner: RetainedServiceOwner::from_token(owner_token),
            connection: Mutex::new(Some(Connection {
                reader: Box::new(std::io::stdin()),
                writer: Box::new(std::io::stdout()),
            })),
            attachments: Mutex::new(HashMap::new()),
        }
    }

    fn exchange(&self, command: Command) -> Result<Reply, ProcessError> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(active) = connection.as_mut() else {
            return Err(disconnected());
        };
        if protocol::write_command(&mut active.writer, &command).is_err() {
            *connection = None;
            return Err(disconnected());
        }
        match protocol::read_reply(&mut active.reader) {
            Ok(Some(reply)) if command_accepts_reply(&command, &reply) => Ok(reply),
            Ok(Some(_)) => {
                // Fence the stream before releasing its serialization lock. Otherwise another
                // caller could submit an effect between decoding the wrong reply and poisoning
                // this child generation.
                *connection = None;
                Err(protocol_error())
            }
            Ok(None) | Err(_) => {
                // A partial request or untagged response loss makes the byte stream ambiguous.
                // Fence this child generation permanently instead of letting a later effect
                // consume a stale reply or replaying the command whose admission is unknown.
                *connection = None;
                Err(disconnected())
            }
        }
    }

    fn unit(&self, command: Command) -> Result<(), ProcessError> {
        match self.exchange(command)? {
            Reply::Unit => Ok(()),
            Reply::Error(error) => Err(error),
            Reply::Observation(_)
            | Reply::StreamPage(_)
            | Reply::Terminal(_)
            | Reply::Pending => Err(protocol_error()),
        }
    }

    fn unit_waiting(&self, mut command: impl FnMut() -> Command) -> Result<(), ProcessError> {
        loop {
            match self.unit(command()) {
                Err(error)
                    if error.control_rejection() == Some(ControlRejection::Backpressure) =>
                {
                    std::thread::yield_now();
                }
                result => return result,
            }
        }
    }

    fn ensure_attached(&self, key: RetainedProcessKey) -> Result<(), ProcessError> {
        let attachment = self.attachment(key)?;
        if attachment.attached {
            return Ok(());
        }
        let mut reply = self.exchange(Command::Launch {
            key,
            request_digest: attachment.digest,
            request: attachment.request,
        })?;
        loop {
            match reply {
                Reply::Unit => break,
                Reply::Pending => {
                    std::thread::sleep(OWNER_POLL_INTERVAL);
                    reply = self.exchange(Command::LaunchStatus {
                        key,
                        request_digest: attachment.digest,
                    })?;
                }
                Reply::Error(error) => return Err(error),
                Reply::Observation(_) | Reply::StreamPage(_) | Reply::Terminal(_) => {
                    return Err(protocol_error());
                }
            }
        }
        self.mark_attached(key);
        Ok(())
    }

    fn attachment(&self, key: RetainedProcessKey) -> Result<Attachment, ProcessError> {
        self.attachments
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .cloned()
            .ok_or_else(|| owner_unavailable("retained owner request is unavailable"))
    }

    fn mark_attached(&self, key: RetainedProcessKey) {
        if let Some(current) = self
            .attachments
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(&key)
        {
            current.attached = true;
        }
    }
}

impl RetainedProcessTransport for OwnerClient {
    fn service_owner(&self) -> RetainedServiceOwner {
        self.service_owner
    }

    fn launch_or_attach(
        &self,
        key: RetainedProcessKey,
        request: &[u8],
        request_digest: Sha256Digest,
    ) -> Result<(), ProcessError> {
        let mut attachments = self
            .attachments
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = attachments.get(&key) {
            if existing.digest != request_digest || existing.request != request {
                return Err(owner_unavailable("retained owner key was rebound to another request"));
            }
        } else {
            attachments.insert(
                key,
                Attachment { request: request.to_vec(), digest: request_digest, attached: false },
            );
        }
        drop(attachments);
        let attachment = self.attachment(key)?;
        match self.exchange(Command::Launch {
            key,
            request_digest: attachment.digest,
            request: attachment.request,
        })? {
            Reply::Unit => self.mark_attached(key),
            Reply::Pending => {}
            Reply::Error(error) => return Err(error),
            Reply::Observation(_) | Reply::StreamPage(_) | Reply::Terminal(_) => {
                return Err(protocol_error());
            }
        }
        Ok(())
    }

    fn write_stdin(
        &self,
        key: RetainedProcessKey,
        bytes: Vec<u8>,
        wait_for_capacity: bool,
    ) -> Result<(), ProcessError> {
        self.ensure_attached(key)?;
        if wait_for_capacity {
            self.unit_waiting(|| Command::Write { key, wait: false, bytes: bytes.clone() })
        } else {
            self.unit(Command::Write { key, wait: false, bytes })
        }
    }

    fn close_stdin(
        &self,
        key: RetainedProcessKey,
        wait_for_capacity: bool,
    ) -> Result<(), ProcessError> {
        self.ensure_attached(key)?;
        if wait_for_capacity {
            self.unit_waiting(|| Command::Close { key, wait: false })
        } else {
            self.unit(Command::Close { key, wait: false })
        }
    }

    fn resize(&self, key: RetainedProcessKey, size: TerminalSize) -> Result<(), ProcessError> {
        self.ensure_attached(key)?;
        self.unit(Command::Resize { key, size })
    }

    fn signal(&self, key: RetainedProcessKey, signal: ProcessSignal) -> Result<(), ProcessError> {
        self.ensure_attached(key)?;
        self.unit(Command::Signal { key, signal })
    }

    fn cancel(
        &self,
        key: RetainedProcessKey,
        reason: CancellationReason,
    ) -> Result<(), ProcessError> {
        let request_digest = self.attachment(key)?.digest;
        self.unit(Command::Cancel { key, request_digest, reason })
    }

    fn observe(
        &self,
        key: RetainedProcessKey,
        cursor: ProcessCursor,
        max_events: usize,
        wait: Option<Duration>,
    ) -> Result<RetainedOwnerObservation, ProcessError> {
        let began = Instant::now();
        loop {
            let observation = match self.exchange(Command::Observe {
                key,
                cursor,
                max_events: max_events.min(OWNER_EVENT_PAGE),
                wait: None,
            })? {
                Reply::Observation(observation) => observation,
                Reply::Error(error) => return Err(error),
                Reply::Unit
                | Reply::StreamPage(_)
                | Reply::Terminal(_)
                | Reply::Pending => return Err(protocol_error()),
            };
            let Some(timeout) = wait else {
                return Ok(observation);
            };
            if !observation.events().is_empty()
                || observation.terminal_result().is_some()
                || observation.owner_finished()
            {
                return Ok(observation);
            }
            let elapsed = began.elapsed();
            if elapsed >= timeout {
                return Ok(observation);
            }
            std::thread::sleep(OWNER_POLL_INTERVAL.min(timeout - elapsed));
        }
    }

    fn read_stream_page(
        &self,
        key: RetainedProcessKey,
        stream: OutputStream,
        snapshot_digest: Option<Sha256Digest>,
        offset: u64,
        max_bytes: usize,
    ) -> Result<RetainedStreamPage, ProcessError> {
        self.ensure_attached(key)?;
        match self.exchange(Command::Stream {
            key,
            stream,
            snapshot_digest,
            offset,
            max_bytes: max_bytes.min(OWNER_STREAM_PAGE_BYTES),
        })? {
            Reply::StreamPage(page) => Ok(page),
            Reply::Error(error) => Err(error),
            Reply::Unit | Reply::Observation(_) | Reply::Terminal(_) | Reply::Pending => {
                Err(protocol_error())
            }
        }
    }

    fn wait(&self, key: RetainedProcessKey) -> Result<TerminalResult, ProcessError> {
        self.ensure_attached(key)?;
        loop {
            match self.exchange(Command::Wait { key })? {
                Reply::Terminal(terminal) => return Ok(terminal),
                Reply::Pending => std::thread::sleep(OWNER_POLL_INTERVAL),
                Reply::Error(error) => return Err(error),
                Reply::Unit | Reply::Observation(_) | Reply::StreamPage(_) => {
                    return Err(protocol_error());
                }
            }
        }
    }
}

fn command_accepts_reply(command: &Command, reply: &Reply) -> bool {
    if matches!(reply, Reply::Error(_)) {
        return true;
    }
    match command {
        Command::Launch { .. } | Command::LaunchStatus { .. } => {
            matches!(reply, Reply::Unit | Reply::Pending)
        }
        Command::Observe { .. } => matches!(reply, Reply::Observation(_)),
        Command::Stream { .. } => matches!(reply, Reply::StreamPage(_)),
        Command::Wait { .. } => matches!(reply, Reply::Terminal(_) | Reply::Pending),
        Command::Write { .. }
        | Command::Close { .. }
        | Command::Resize { .. }
        | Command::Signal { .. }
        | Command::Cancel { .. } => matches!(reply, Reply::Unit),
    }
}

const fn disconnected() -> ProcessError {
    owner_unavailable("retained service owner connection was interrupted")
}

const fn protocol_error() -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        "retained service owner returned another response kind",
    )
}

const fn owner_unavailable(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Supervisor,
        ProcessOperation::Reconcile,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}
