//! Process-owned extension point for authorized native sandbox sessions.

mod observation;
mod probe;
mod protected_handle;
mod protocol;
#[cfg(unix)]
mod pty;
#[cfg(windows)]
mod windows_channel;

pub use probe::NativeProcessProbe;
pub use protected_handle::NativeProtectedHandle;
pub use protocol::{
    native_activation_record, native_ready_record, native_target_exec_failed_record,
    native_target_started_record,
};
pub use observation::{
    native_observation_prefix_digest, native_observation_producer_binding,
};
#[cfg(unix)]
pub use pty::{NATIVE_PTY_SLAVE_ENV, NativePtyAttachment};
#[cfg(windows)]
pub use windows_channel::{
    NATIVE_WINDOWS_CONTROL_HANDLE_ENV, NATIVE_WINDOWS_STATUS_HANDLE_ENV,
    NativeWindowsHelperAttachment, NativeWindowsHelperChannels,
};

use peritus_sandbox::{
    BackendAdmission, BackendDescriptor, CheckedSandboxPlan, EnforcementObservation,
};
use peritus_types::Sha256Digest;

use crate::{
    CancellationReason, CommandSpec, ErrorCode, ExecutionPlan, OsExitObservation, ProcessError,
    ProcessOperation, ProcessTreeIdentity, RecoveryClass, RetainedBackendFactoryRequest,
    RetainedOwnerBinding,
};

pub(crate) use observation::{
    capture_activated_session, capture_prepared_session, capture_released_session,
    capture_released_pre_start_session, capture_terminated_session,
};

const MAX_HELPER_IDENTITY_BYTES: usize = 256;
/// Maximum number of exact observations transferred in one physical page.
pub const NATIVE_OBSERVATION_PAGE_RECORDS: usize = 256;

/// Closed lifecycle vocabulary carried by an independently retained native session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativeRecoveryPhase {
    /// Native resources are prepared; the helper may or may not have been spawned yet.
    Prepared,
    /// Helper activation and exact tree custody were accepted.
    Active,
    /// The immutable first cancellation reason was accepted.
    Cancelling,
    /// Root termination was observed while native cleanup remains owned.
    Terminated,
    /// Every backend-owned resource was released.
    Released,
}

/// Exact live native-session evidence published only by the retained process owner.
///
/// The opaque record remains platform-owned. The common process layer binds it to the consumed
/// retained-owner operation and exact native birth identity before allowing attachment to count
/// as adoption. A record without retained custody remains diagnostic and cannot authorize a
/// recovered live-owner claim.
#[derive(Clone, Eq, PartialEq)]
pub struct NativeSessionRecovery {
    platform: NativePlatform,
    process_id: peritus_types::ProcessId,
    phase: NativeRecoveryPhase,
    tree: Option<ProcessTreeIdentity>,
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
    custody_complete: bool,
    record: Vec<u8>,
    record_digest: Sha256Digest,
}

impl core::fmt::Debug for NativeSessionRecovery {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("NativeSessionRecovery")
            .field("platform", &self.platform)
            .field("process_id", &self.process_id)
            .field("phase", &self.phase)
            .field("tree", &self.tree)
            .field("custody_complete", &self.custody_complete)
            .field("record_bytes", &self.record.len())
            .field("record_digest", &self.record_digest)
            .finish_non_exhaustive()
    }
}

impl NativeSessionRecovery {
    /// Creates one bounded live-owner snapshot from a platform-authenticated record.
    ///
    /// # Errors
    /// Rejects incomplete retained-owner bindings, invalid live-tree identity, and unbounded or
    /// empty platform records.
    #[allow(clippy::too_many_arguments, reason = "each adoption binding remains explicit")]
    pub fn new(
        platform: NativePlatform,
        process_id: peritus_types::ProcessId,
        phase: NativeRecoveryPhase,
        tree: Option<ProcessTreeIdentity>,
        owner_operation_digest: Option<Sha256Digest>,
        service_owner_digest: Option<Sha256Digest>,
        custody_complete: bool,
        record: Vec<u8>,
    ) -> Result<Self, ProcessError> {
        let retained_binding_complete =
            owner_operation_digest.is_some() == service_owner_digest.is_some();
        let tree_exact = tree.is_none_or(|tree| {
            let platform_tree = match platform {
                NativePlatform::Windows => tree.process_group().is_none(),
                NativePlatform::Linux | NativePlatform::Macos => {
                    tree.process_group() == Some(tree.root_pid())
                }
            };
            tree.root_pid() != 0
                && tree.start_token().is_some()
                && platform_tree
                && tree.complete_containment()
        });
        if record.is_empty()
            || u32::try_from(record.len()).is_err()
            || !retained_binding_complete
            || custody_complete && owner_operation_digest.is_none()
            || !tree_exact
            || matches!(
                phase,
                NativeRecoveryPhase::Active
                    | NativeRecoveryPhase::Cancelling
                    | NativeRecoveryPhase::Terminated
            ) && tree.is_none()
        {
            return Err(ProcessError::new(
                ErrorCode::CorruptRecovery,
                ProcessOperation::Reconcile,
                RecoveryClass::Quarantine,
                "native session recovery snapshot is invalid",
            ));
        }
        let record_digest = peritus_codec::sha256(&record);
        Ok(Self {
            platform,
            process_id,
            phase,
            tree,
            owner_operation_digest,
            service_owner_digest,
            custody_complete,
            record,
            record_digest,
        })
    }

    /// Returns the native platform that owns the opaque record.
    #[must_use]
    pub const fn platform(&self) -> NativePlatform { self.platform }
    /// Returns the consumed execution identity.
    #[must_use]
    pub const fn process_id(&self) -> peritus_types::ProcessId { self.process_id }
    /// Returns the platform session phase.
    #[must_use]
    pub const fn phase(&self) -> NativeRecoveryPhase { self.phase }
    /// Returns the exact accepted helper birth identity, when spawned.
    #[must_use]
    pub const fn tree_identity(&self) -> Option<ProcessTreeIdentity> { self.tree }
    /// Returns the retained owner operation binding, when adoption is available.
    #[must_use]
    pub const fn owner_operation_digest(&self) -> Option<Sha256Digest> {
        self.owner_operation_digest
    }
    /// Returns the authenticated retained service-owner generation.
    #[must_use]
    pub const fn service_owner_digest(&self) -> Option<Sha256Digest> {
        self.service_owner_digest
    }
    /// Reports that every resource required by the current phase remains under the live owner.
    #[must_use]
    pub const fn custody_complete(&self) -> bool { self.custody_complete }
    /// Borrows the platform-owned canonical record.
    #[must_use]
    pub fn record(&self) -> &[u8] { &self.record }
    /// Returns the digest of the complete platform record.
    #[must_use]
    pub const fn record_digest(&self) -> Sha256Digest { self.record_digest }

    /// Verifies that this live snapshot belongs to one exact retained-owner request.
    #[must_use]
    pub fn matches_retained_owner(
        &self,
        platform: NativePlatform,
        binding: RetainedOwnerBinding,
    ) -> bool {
        self.platform == platform
            && self.process_id == binding.process_id()
            && self.owner_operation_digest == Some(binding.operation_digest())
            && self.service_owner_digest == Some(binding.service_owner().digest())
            && self.custody_complete
            && self.phase != NativeRecoveryPhase::Released
    }
}

/// Observation transport exposed by a native session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativeObservationTransport {
    /// Compatibility view for producers that still expose one in-memory snapshot.
    LegacySnapshot,
    /// Stable unacknowledged deltas retained until C2 durably adopts their receipt.
    DurableDelta,
}

/// One bounded transfer from a native session's exact observation stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeObservationPage {
    head_sequence: u64,
    observations: Vec<EnforcementObservation>,
}

impl NativeObservationPage {
    /// Creates a bounded page and declares the stable stream head observed with it.
    ///
    /// # Errors
    /// Rejects a page larger than the physical transfer bound or a record beyond the declared
    /// head. Sequence continuity and binding are checked by the durable C2 tracker.
    pub fn new(
        head_sequence: u64,
        observations: Vec<EnforcementObservation>,
    ) -> Result<Self, ProcessError> {
        if observations.len() > NATIVE_OBSERVATION_PAGE_RECORDS
            || observations.last().is_some_and(|value| value.sequence() > head_sequence)
        {
            return Err(native_mismatch("native observation page framing is invalid"));
        }
        Ok(Self { head_sequence, observations })
    }

    /// Returns the stable last sequence available from the producer.
    #[must_use]
    pub const fn head_sequence(&self) -> u64 {
        self.head_sequence
    }

    /// Returns this physical page's records in producer order.
    #[must_use]
    pub fn observations(&self) -> &[EnforcementObservation] {
        &self.observations
    }

    pub(crate) fn into_observations(self) -> Vec<EnforcementObservation> {
        self.observations
    }
}

/// Exact durable frontier C2 returns after adopting one native observation page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeObservationReceipt {
    through_sequence: u64,
    page_digest: Sha256Digest,
    producer_binding_digest: Sha256Digest,
    producer_prefix_digest: Sha256Digest,
}

impl NativeObservationReceipt {
    pub(crate) const fn new(
        through_sequence: u64,
        page_digest: Sha256Digest,
        producer_binding_digest: Sha256Digest,
        producer_prefix_digest: Sha256Digest,
    ) -> Self {
        Self {
            through_sequence,
            page_digest,
            producer_binding_digest,
            producer_prefix_digest,
        }
    }

    /// Returns the last sequence durably adopted by C2.
    #[must_use]
    pub const fn through_sequence(self) -> u64 {
        self.through_sequence
    }

    /// Returns the digest of the immutable page that established the frontier.
    #[must_use]
    pub const fn page_digest(self) -> Sha256Digest {
        self.page_digest
    }

    /// Returns the exact prepared producer identity bound to the receipt.
    #[must_use]
    pub const fn producer_binding_digest(self) -> Sha256Digest {
        self.producer_binding_digest
    }

    /// Returns the chained digest of every producer record through this receipt.
    #[must_use]
    pub const fn producer_prefix_digest(self) -> Sha256Digest {
        self.producer_prefix_digest
    }
}

/// Native operating-system family implemented by a restricted backend.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativePlatform {
    /// Linux namespaces and kernel controls.
    Linux,
    /// macOS Seatbelt and process controls.
    Macos,
    /// Windows token, `AppContainer`, and job controls.
    Windows,
}

impl NativePlatform {
    /// Returns the platform on which this process crate was compiled.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            panic!("peritus-process supports only Linux, macOS, and Windows");
        }
    }
}

/// Structured direct-child command produced by one authorized native preparation.
#[derive(Clone, Debug)]
pub struct NativeLaunchDescription {
    command: CommandSpec,
    helper_identity: String,
    manifest: Vec<u8>,
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    protected_handles: Vec<NativeProtectedHandle>,
    #[cfg(windows)]
    windows_helper_channels: Option<NativeWindowsHelperChannels>,
}

impl NativeLaunchDescription {
    /// Creates a digest-bound helper launch description without a shell command line.
    ///
    /// # Errors
    ///
    /// Rejects an empty, oversized, non-ASCII, or control-bearing helper identity.
    pub fn new(
        command: CommandSpec,
        helper_identity: impl Into<String>,
        manifest: Vec<u8>,
        manifest_digest: Sha256Digest,
        preparation_digest: Sha256Digest,
    ) -> Result<Self, ProcessError> {
        let helper_identity = helper_identity.into();
        if helper_identity.is_empty()
            || helper_identity.len() > MAX_HELPER_IDENTITY_BYTES
            || !helper_identity.is_ascii()
            || helper_identity.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(native_mismatch("native helper identity is invalid or exceeds its bound"));
        }
        if manifest.is_empty() || u32::try_from(manifest.len()).is_err() {
            return Err(native_mismatch(
                "native helper manifest is empty or exceeds protected frame capacity",
            ));
        }
        if peritus_codec::sha256(&manifest) != manifest_digest {
            return Err(native_mismatch("native helper manifest digest does not match its bytes"));
        }
        Ok(Self {
            command,
            helper_identity,
            manifest,
            manifest_digest,
            preparation_digest,
            protected_handles: Vec::new(),
            #[cfg(windows)]
            windows_helper_channels: None,
        })
    }

    /// Adds the exact protected anonymous handles inherited by the native helper.
    ///
    /// Handle values remain unchanged across the helper launch so the backend manifest can bind
    /// each value to its exact proxy-token or secret-delivery destination. C2 retains ownership
    /// through the native session and enables inheritance only in the direct child.
    ///
    /// # Errors
    ///
    /// Rejects duplicate labels or duplicate operating-system handles.
    pub fn with_protected_handles(
        self,
        mut handles: Vec<NativeProtectedHandle>,
    ) -> Result<Self, ProcessError> {
        handles.sort_by(|left, right| left.label().cmp(right.label()));
        self.attach_protected_handles(handles)
    }

    /// Adds exact protected handles while retaining the caller's validated role order.
    ///
    /// Platform backends use this after comparing every handle with an ordered helper manifest.
    /// The operating system remains the cumulative capacity authority; this representation adds
    /// no independent total-count ceiling.
    ///
    /// # Errors
    /// Rejects duplicate labels or duplicate operating-system handles.
    pub fn with_ordered_protected_handles(
        self,
        handles: Vec<NativeProtectedHandle>,
    ) -> Result<Self, ProcessError> {
        self.attach_protected_handles(handles)
    }

    fn attach_protected_handles(
        mut self,
        handles: Vec<NativeProtectedHandle>,
    ) -> Result<Self, ProcessError> {
        let mut labels = handles.iter().map(NativeProtectedHandle::label).collect::<Vec<_>>();
        labels.sort_unstable();
        if labels.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(native_mismatch("native protected handle labels collide"));
        }
        let mut raw_handles =
            handles.iter().map(NativeProtectedHandle::raw_handle).collect::<Vec<_>>();
        raw_handles.sort_unstable();
        if raw_handles.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(native_mismatch("native protected operating-system handles collide"));
        }
        self.protected_handles = handles;
        Ok(self)
    }

    /// Adds C2-owned protected status and resize channels for the Windows helper.
    ///
    /// # Errors
    /// Rejects collisions with already staged protected handles.
    #[cfg(windows)]
    pub fn with_windows_helper_channels(
        mut self,
        mut channels: NativeWindowsHelperChannels,
    ) -> Result<Self, ProcessError> {
        let mut handles = core::mem::take(&mut self.protected_handles);
        handles.extend(channels.take_child_handles());
        self = self.with_protected_handles(handles)?;
        self.windows_helper_channels = Some(channels);
        Ok(self)
    }

    /// Returns the literal executable and argv used for the direct child.
    #[must_use]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }

    /// Returns the reviewed helper implementation identity.
    #[must_use]
    pub fn helper_identity(&self) -> &str {
        &self.helper_identity
    }

    /// Returns the binary manifest written to the helper's inherited input stream.
    ///
    /// The helper consumes the length-prefixed frame before forwarding any subsequent target
    /// input. Secret values are never part of this manifest.
    #[must_use]
    pub fn manifest(&self) -> &[u8] {
        &self.manifest
    }

    /// Returns the digest of the protected-frame helper manifest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }

    /// Returns the admitted native preparation digest.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }

    /// Returns the exact protected handles retained for the helper launch.
    #[must_use]
    pub fn protected_handles(&self) -> &[NativeProtectedHandle] {
        &self.protected_handles
    }

    /// Releases the parent-side ownership of one exact protected child handle.
    ///
    /// Returns whether a matching handle was present. Native sessions use this after the direct
    /// child has inherited a close-on-exec status writer, allowing the retained reader to observe
    /// target-exec success as EOF.
    pub fn release_protected_handle(&mut self, label: &str) -> bool {
        let Some(position) =
            self.protected_handles.iter().position(|handle| handle.label() == label)
        else {
            return false;
        };
        self.protected_handles.remove(position);
        true
    }

    #[cfg(windows)]
    pub(crate) const fn windows_helper_channels(&self) -> Option<&NativeWindowsHelperChannels> {
        self.windows_helper_channels.as_ref()
    }

    /// Returns the fixed record the helper writes after opening its protected channels.
    #[must_use]
    pub fn ready_record(&self) -> Sha256Digest {
        native_ready_record()
    }

    /// Returns the fixed record the helper writes after installing every admitted control.
    #[must_use]
    pub fn activation_record(&self) -> Sha256Digest {
        native_activation_record(self.manifest_digest, self.preparation_digest)
    }
}

/// Opaque post-consumption view passed only by [`crate::ExecutionGateway`].
///
/// Callers can inspect the exact plans while implementing a backend, but cannot construct this
/// value and therefore cannot invoke an authorized preparation independently.
pub struct AuthorizedPreparationContext<'a> {
    execution_plan: &'a ExecutionPlan,
    sandbox_plan: &'a CheckedSandboxPlan,
    admission: &'a BackendAdmission,
    retained_owner: Option<RetainedOwnerBinding>,
}

impl<'a> AuthorizedPreparationContext<'a> {
    pub(crate) const fn new(
        execution_plan: &'a ExecutionPlan,
        sandbox_plan: &'a CheckedSandboxPlan,
        admission: &'a BackendAdmission,
    ) -> Self {
        Self { execution_plan, sandbox_plan, admission, retained_owner: None }
    }

    pub(crate) const fn retained(
        execution_plan: &'a ExecutionPlan,
        sandbox_plan: &'a CheckedSandboxPlan,
        admission: &'a BackendAdmission,
        retained_owner: RetainedOwnerBinding,
    ) -> Self {
        Self {
            execution_plan,
            sandbox_plan,
            admission,
            retained_owner: Some(retained_owner),
        }
    }

    /// Returns the exact authorized execution plan.
    #[must_use]
    pub const fn execution_plan(&self) -> &ExecutionPlan {
        self.execution_plan
    }

    /// Returns the exact checked sandbox plan.
    #[must_use]
    pub const fn sandbox_plan(&self) -> &CheckedSandboxPlan {
        self.sandbox_plan
    }

    /// Returns the exact admitted backend facts.
    #[must_use]
    pub const fn admission(&self) -> &BackendAdmission {
        self.admission
    }

    /// Returns the exact retained-owner binding when preparation runs inside that service owner.
    ///
    /// `None` is an explicit local-owner session and cannot later claim retained adoption.
    #[must_use]
    pub const fn retained_owner(&self) -> Option<RetainedOwnerBinding> {
        self.retained_owner
    }
}

/// Native backend called by the process gateway only after exact validation and durable consume.
///
/// Constructing a backend may perform transient support probes, but every probe-owned worker or
/// process must be fully finished and joined before backend construction returns. Construction
/// must not create a prepared session, start session support tasks, or retain live preparation
/// ownership. Prepared-session resource ownership begins only in [`Self::prepare`].
pub trait NativeSandboxBackend: Send + 'static {
    /// Prepared session retained by the process supervisor through teardown.
    type Session: NativeSandboxSession;

    /// Returns the probed descriptor used by this implementation.
    fn descriptor(&self) -> &BackendDescriptor;

    /// Returns the operating-system family this implementation enforces.
    fn platform(&self) -> NativePlatform;

    /// Freezes the nonsensitive trusted-config identity needed to reconstruct this backend in the
    /// retained service owner.
    ///
    /// Implementations must bind the supplied checked plan and exact admission, including the
    /// descriptor, support, and preparation digests. The payload may identify approved helper,
    /// controller, or resolver configuration, but must not contain live handles, routing tokens,
    /// credential material, or secret values.
    ///
    /// # Errors
    ///
    /// Returns a typed pre-effect failure when this backend cannot be reconstructed by the
    /// independently retained owner.
    fn retained_factory_request(
        &self,
        _sandbox: &CheckedSandboxPlan,
        _admission: &BackendAdmission,
    ) -> Result<RetainedBackendFactoryRequest, ProcessError> {
        Err(native_mismatch(
            "native backend does not support retained owner reconstruction",
        ))
    }

    /// Validates current platform launch capacity before fresh one-use authority is consumed.
    ///
    /// Implementations may inspect only current nonsensitive platform capacity and the already
    /// checked sandbox plan. This preflight must not create a prepared session, reserve live
    /// handles, or start support work. Exact handle identities are checked again during
    /// post-consumption preparation.
    ///
    /// # Errors
    /// Returns a typed pre-effect failure when current platform capacity cannot represent the
    /// selected helper contract.
    fn validate_preparation_capacity(
        &self,
        _sandbox: &CheckedSandboxPlan,
    ) -> Result<(), ProcessError> {
        Ok(())
    }

    /// Prepares one session from the opaque authorized context.
    ///
    /// This is the first operation permitted to create session support tasks or other live
    /// preparation resources. A backend that has not entered this method remains inert under the
    /// trait contract above.
    ///
    /// # Errors
    ///
    /// Returns a typed failure without starting the target when native preparation cannot be
    /// completed exactly.
    fn prepare(
        self,
        context: AuthorizedPreparationContext<'_>,
    ) -> Result<Self::Session, ProcessError>;
}

/// Native lifecycle state owned by the existing C2 supervisor.
pub trait NativeSandboxSession: Send + 'static {
    /// Returns the exact helper/direct-child launch description.
    fn launch_description(&self) -> &NativeLaunchDescription;

    /// Returns exact platform recovery evidence held by the current live owner.
    ///
    /// The default makes custody unavailable explicitly. Retained-owner attachment never treats
    /// an absent snapshot as a released resource or as authority to replace the native session.
    ///
    /// # Errors
    /// Returns a typed recovery failure when the platform record cannot be represented exactly.
    fn recovery_snapshot(&self) -> Result<Option<NativeSessionRecovery>, ProcessError> {
        Ok(None)
    }

    /// Retains the exact helper birth and process-tree identity before protocol delivery begins.
    ///
    /// The default is appropriate for backends whose durable state is owned entirely by C2.
    /// Backends that keep their own recovery record override this to bind the one spawned helper
    /// before any manifest bytes can permit activation.
    ///
    /// # Errors
    /// Returns a typed fail-closed error when the exact spawned identity cannot be retained.
    fn spawned(&mut self, _tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        Ok(())
    }

    /// Returns a bounded diagnostic tail of ordered, plan-bound native observations.
    ///
    /// This compatibility view does not establish a durable receipt. New implementations should
    /// additionally expose [`NativeObservationTransport::DurableDelta`] through
    /// [`Self::observation_page`] and [`Self::acknowledge_observations`].
    fn observations(&self) -> &[EnforcementObservation];

    /// Returns how many common observations precede the retained diagnostic tail.
    #[must_use]
    fn observation_tail_dropped(&self) -> u64 {
        0
    }

    /// Returns the latest exact durable receipt accepted by this producer.
    #[must_use]
    fn acknowledged_observation_receipt(&self) -> Option<NativeObservationReceipt> {
        None
    }

    /// Selects exact delta persistence or the explicit legacy snapshot adapter.
    #[must_use]
    fn observation_transport(&self) -> NativeObservationTransport {
        NativeObservationTransport::LegacySnapshot
    }

    /// Reads at most one physical page strictly after `after_sequence`.
    ///
    /// The default adapter traverses the complete legacy snapshot in bounded transfers and has no
    /// cumulative observation ceiling. It remains explicitly legacy because the producer cannot
    /// retain unacknowledged deltas after C2 returns.
    ///
    /// # Errors
    /// Returns a typed failure when the page cannot be produced.
    fn observation_page(
        &self,
        after_sequence: u64,
    ) -> Result<NativeObservationPage, ProcessError> {
        let observations = self.observations();
        let head_sequence = observations.last().map_or(0, |value| value.sequence());
        let page = observations
            .iter()
            .copied()
            .filter(|value| value.sequence() > after_sequence)
            .take(NATIVE_OBSERVATION_PAGE_RECORDS)
            .collect();
        NativeObservationPage::new(head_sequence, page)
    }

    /// Releases producer memory through one exact durable receipt.
    ///
    /// The legacy adapter intentionally performs no acknowledgement. Implementations selecting
    /// durable deltas must override this method and reject receipts that do not match their pending
    /// prefix.
    ///
    /// # Errors
    /// Returns a typed failure when a durable producer cannot accept the receipt exactly.
    fn acknowledge_observations(
        &mut self,
        _receipt: NativeObservationReceipt,
    ) -> Result<(), ProcessError> {
        if self.observation_transport() == NativeObservationTransport::DurableDelta {
            return Err(native_mismatch(
                "durable native observation producer lacks acknowledgement support",
            ));
        }
        Ok(())
    }

    /// Polls backend-owned supervisor resource dimensions for the live tree.
    ///
    /// Hard-enforcement backends keep the default. Backends that truthfully advertise supervisor
    /// enforcement return [`NativePoll::ResourceLimitExceeded`] as soon as a checked ceiling is
    /// crossed; C2 then owns the ordinary first-trigger cancellation and reap path.
    ///
    /// # Errors
    /// Returns a typed fail-closed observation failure.
    fn poll_resources(&mut self, _tree: ProcessTreeIdentity) -> Result<NativePoll, ProcessError> {
        Ok(NativePoll::Continue)
    }

    /// Records successful target-tree activation.
    ///
    /// # Errors
    /// Returns a typed fail-closed backend error.
    fn activated(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError>;

    /// Records activation while the durable process owner continues to permit protocol I/O.
    ///
    /// Backends with a post-handshake execution acknowledgement override this method. The
    /// default checks cancellation once and then uses the backend's immediate activation path.
    ///
    /// # Errors
    /// Returns a typed fail-closed backend error or an owner-cancellation error.
    fn activated_while(
        &mut self,
        tree: ProcessTreeIdentity,
        should_continue: &mut dyn FnMut() -> bool,
    ) -> Result<(), ProcessError> {
        if !should_continue() {
            return Err(ProcessError::new(
                ErrorCode::Supervisor,
                ProcessOperation::Wait,
                RecoveryClass::CancelAndReap,
                "native activation was cancelled by its durable owner",
            ));
        }
        self.activated(tree)
    }

    /// Records the first accepted cancellation request.
    ///
    /// # Errors
    /// Returns a typed fail-closed backend error.
    fn cancellation_requested(&mut self, reason: CancellationReason) -> Result<(), ProcessError>;

    /// Records the observed root termination.
    ///
    /// # Errors
    /// Returns a typed fail-closed backend error.
    fn terminated(&mut self, exit: &OsExitObservation) -> Result<(), ProcessError>;

    /// Releases every backend-owned native, network, and secret resource.
    ///
    /// # Errors
    /// Returns a typed fail-closed error when complete release cannot be established.
    fn release(&mut self) -> Result<(), ProcessError>;
}

/// Result of one backend-owned supervisor resource sample.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativePoll {
    /// No checked native supervisor ceiling has been crossed.
    Continue,
    /// At least one checked native supervisor ceiling has been crossed.
    ResourceLimitExceeded,
}

const fn native_mismatch(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::PlanMismatch,
        ProcessOperation::Validate,
        RecoveryClass::SelectBackend,
        detail,
    )
}
