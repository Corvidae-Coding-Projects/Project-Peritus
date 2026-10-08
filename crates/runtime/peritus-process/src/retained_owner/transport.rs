//! Process-side contract for an independently retained native owner.

use std::time::Duration;

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    CancellationReason, OutputStream, ProcessCursor, ProcessError, ProcessEvent, ProcessSignal,
    NativeSessionRecovery, ProcessTreeIdentity, TerminalResult, TerminalSize,
};

use super::{RetainedOwnerBinding, RetainedOwnerNonce, RetainedServiceOwner};

/// Exact stable identity used for every retained-owner control and observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RetainedProcessKey {
    process_id: ProcessId,
    nonce: RetainedOwnerNonce,
    operation_digest: Sha256Digest,
}

/// Claim-linked canonical owner request recovered from the protected process registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedOwnerReservation {
    operation_digest: Sha256Digest,
    request_digest: Sha256Digest,
    request: Vec<u8>,
    phase: crate::LifecyclePhase,
}

impl RetainedOwnerReservation {
    pub(crate) const fn new(
        operation_digest: Sha256Digest,
        request_digest: Sha256Digest,
        request: Vec<u8>,
        phase: crate::LifecyclePhase,
    ) -> Self {
        Self { operation_digest, request_digest, request, phase }
    }

    /// Returns the exact owner-operation digest bound by the one-use claim.
    #[must_use]
    pub const fn operation_digest(&self) -> Sha256Digest { self.operation_digest }
    /// Returns the digest of the complete canonical request bytes.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest { self.request_digest }
    /// Returns the complete canonical request bytes retained before authority consumption.
    #[must_use]
    pub fn request(&self) -> &[u8] { &self.request }
    /// Returns the latest durable C2 lifecycle phase for this retained execution.
    #[must_use]
    pub const fn phase(&self) -> crate::LifecyclePhase { self.phase }
}

impl RetainedProcessKey {
    /// Derives the control identity from the complete pre-effect owner binding.
    #[must_use]
    pub const fn from_binding(binding: RetainedOwnerBinding) -> Self {
        Self {
            process_id: binding.process_id(),
            nonce: binding.nonce(),
            operation_digest: binding.operation_digest(),
        }
    }

    /// Restores an authenticated key carried by the protected owner protocol.
    #[must_use]
    pub const fn new(
        process_id: ProcessId,
        nonce: RetainedOwnerNonce,
        operation_digest: Sha256Digest,
    ) -> Self {
        Self { process_id, nonce, operation_digest }
    }

    /// Returns the exact process identity.
    #[must_use]
    pub const fn process_id(self) -> ProcessId { self.process_id }
    /// Returns the reservation nonce.
    #[must_use]
    pub const fn nonce(self) -> RetainedOwnerNonce { self.nonce }
    /// Returns the complete owner-operation digest.
    #[must_use]
    pub const fn operation_digest(self) -> Sha256Digest { self.operation_digest }
}

/// One exact observation returned by the retained process owner.
#[derive(Clone, Debug)]
pub struct RetainedOwnerObservation {
    events: Vec<ProcessEvent>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    terminal_output: Vec<u8>,
    terminal: Option<TerminalResult>,
    owner_finished: bool,
    tree: Option<ProcessTreeIdentity>,
    native_recovery: Option<NativeSessionRecovery>,
}

/// One physical page from an exact retained stream-tail snapshot.
///
/// The snapshot digest and total length remain stable across pages. A caller that cannot retain
/// the complete tail may process each page and advance by [`Self::next_offset`] without imposing
/// a lifetime output allowance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedStreamPage {
    stream: OutputStream,
    snapshot_digest: Sha256Digest,
    total_bytes: u64,
    offset: u64,
    next_offset: u64,
    bytes: Vec<u8>,
}

impl RetainedStreamPage {
    /// Constructs one authenticated physical stream page.
    ///
    /// # Errors
    /// Rejects pages outside the declared snapshot or empty pages before its end.
    pub fn new(
        stream: OutputStream,
        snapshot_digest: Sha256Digest,
        total_bytes: u64,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<Self, ProcessError> {
        let length = u64::try_from(bytes.len()).map_err(|_| invalid_stream_page())?;
        let next = offset.checked_add(length).ok_or_else(invalid_stream_page)?;
        if offset > total_bytes || next > total_bytes || (bytes.is_empty() && offset != total_bytes)
        {
            return Err(invalid_stream_page());
        }
        Ok(Self { stream, snapshot_digest, total_bytes, offset, next_offset: next, bytes })
    }

    /// Returns the exact stream represented by this page.
    #[must_use]
    pub const fn stream(&self) -> OutputStream { self.stream }
    /// Returns the digest of the complete stable tail snapshot.
    #[must_use]
    pub const fn snapshot_digest(&self) -> Sha256Digest { self.snapshot_digest }
    /// Returns the complete snapshot length.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 { self.total_bytes }
    /// Returns the first byte offset in this page.
    #[must_use]
    pub const fn offset(&self) -> u64 { self.offset }
    /// Returns the physical page bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] { &self.bytes }
    /// Returns the exclusive byte offset after this page.
    #[must_use]
    pub const fn next_offset(&self) -> u64 { self.next_offset }
    /// Returns whether this page reaches the snapshot frontier.
    #[must_use]
    pub fn complete(&self) -> bool { self.next_offset() == self.total_bytes }
}

impl RetainedOwnerObservation {
    /// Creates one observation after the transport has authenticated its complete response.
    #[allow(clippy::too_many_arguments, reason = "each retained process observation is explicit")]
    #[must_use]
    pub const fn new(
        events: Vec<ProcessEvent>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        terminal_output: Vec<u8>,
        terminal: Option<TerminalResult>,
        owner_finished: bool,
        tree: Option<ProcessTreeIdentity>,
        native_recovery: Option<NativeSessionRecovery>,
    ) -> Self {
        Self {
            events,
            stdout,
            stderr,
            terminal_output,
            terminal,
            owner_finished,
            tree,
            native_recovery,
        }
    }

    /// Returns the exact bounded event page.
    #[must_use]
    pub fn events(&self) -> &[ProcessEvent] { &self.events }
    /// Returns the current retained tail for one exact stream.
    #[must_use]
    pub fn stream(&self, stream: OutputStream) -> &[u8] {
        match stream {
            OutputStream::Stdout => &self.stdout,
            OutputStream::Stderr => &self.stderr,
            OutputStream::Terminal => &self.terminal_output,
        }
    }
    /// Returns the unique terminal result after durable publication.
    #[must_use]
    pub const fn terminal_result(&self) -> Option<&TerminalResult> { self.terminal.as_ref() }
    /// Returns whether the independently retained supervisor owner has fully exited.
    #[must_use]
    pub const fn owner_finished(&self) -> bool { self.owner_finished }
    /// Returns the exact native process-tree identity after successful launch.
    #[must_use]
    pub const fn tree_identity(&self) -> Option<ProcessTreeIdentity> { self.tree }
    /// Returns exact live native-session recovery evidence from the retained owner.
    #[must_use]
    pub const fn native_recovery(&self) -> Option<&NativeSessionRecovery> {
        self.native_recovery.as_ref()
    }

    /// Validates live native custody separately from an already finished terminal owner.
    #[must_use]
    pub fn matches_native_adoption(
        &self,
        platform: crate::NativePlatform,
        binding: RetainedOwnerBinding,
    ) -> bool {
        if self.owner_finished {
            return self
                .terminal
                .as_ref()
                .is_some_and(|terminal| terminal.process_id() == binding.process_id());
        }
        self.native_recovery.as_ref().is_some_and(|recovery| {
            recovery.matches_retained_owner(platform, binding)
                && recovery.tree_identity() == self.tree
        })
    }
}

/// Protected request/observation transport implemented by the service-supervisor client.
///
/// Implementations authenticate one service-owner generation and serialize all effectful commands
/// so a response loss cannot cause a control to be admitted twice. Transport disconnect is not a
/// cancellation request; a replacement daemon may attach with the same exact key.
pub trait RetainedProcessTransport: Send + Sync + 'static {
    /// Returns the authenticated durable identity of this service-owner generation.
    fn service_owner(&self) -> RetainedServiceOwner;

    /// Admits or reattaches one already-consumed retained request.
    ///
    /// Returning success means the exact service owner retained the request. Backend preparation
    /// or native owner creation may still be pending; observation and cancellation remain usable
    /// through the same key during that interval.
    fn launch_or_attach(
        &self,
        key: RetainedProcessKey,
        request: &[u8],
        request_digest: Sha256Digest,
    ) -> Result<(), ProcessError>;

    /// Sends one exact stdin write, optionally waiting for owner queue capacity.
    fn write_stdin(
        &self,
        key: RetainedProcessKey,
        bytes: Vec<u8>,
        wait_for_capacity: bool,
    ) -> Result<(), ProcessError>;

    /// Closes stdin, optionally waiting for owner queue capacity.
    fn close_stdin(
        &self,
        key: RetainedProcessKey,
        wait_for_capacity: bool,
    ) -> Result<(), ProcessError>;

    /// Applies one checked terminal resize.
    fn resize(&self, key: RetainedProcessKey, size: TerminalSize) -> Result<(), ProcessError>;

    /// Delivers one non-cancelling portable signal.
    fn signal(&self, key: RetainedProcessKey, signal: ProcessSignal) -> Result<(), ProcessError>;

    /// Admits one idempotent cancellation reason.
    fn cancel(
        &self,
        key: RetainedProcessKey,
        reason: CancellationReason,
    ) -> Result<(), ProcessError>;

    /// Returns one bounded exact page and the current owner snapshot.
    fn observe(
        &self,
        key: RetainedProcessKey,
        cursor: ProcessCursor,
        max_events: usize,
        wait: Option<Duration>,
    ) -> Result<RetainedOwnerObservation, ProcessError>;

    /// Returns one bounded physical page from a stable retained stream-tail snapshot.
    ///
    /// `None` starts a fresh snapshot. Later pages supply the returned digest and exact next
    /// offset, so concurrent output cannot splice two different tails into one result.
    fn read_stream_page(
        &self,
        key: RetainedProcessKey,
        stream: OutputStream,
        snapshot_digest: Option<Sha256Digest>,
        offset: u64,
        max_bytes: usize,
    ) -> Result<RetainedStreamPage, ProcessError>;

    /// Waits for the unique retained terminal result without changing ownership.
    fn wait(&self, key: RetainedProcessKey) -> Result<TerminalResult, ProcessError>;
}

const fn invalid_stream_page() -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::CorruptRecovery,
        crate::ProcessOperation::Reconcile,
        crate::RecoveryClass::Quarantine,
        "retained stream page is inconsistent",
    )
}
