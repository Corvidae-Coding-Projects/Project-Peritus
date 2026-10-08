//! Versioned durable runtime identity and cleanup records.

use peritus_process::{
    CancellationReason, NativePlatform, NativeRecoveryPhase, NativeSessionRecovery,
    RetainedOwnerBinding,
};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{MacosError, SessionPhase, TerminationReason};

mod codec;

const MAGIC: [u8; 8] = *b"PRTSMRC1";
const LEGACY_VERSION: u16 = 1;
const PROCESS_BIRTH_VERSION: u16 = 2;
const FILE_CLEANUP_VERSION: u16 = 3;
const CUSTODY_VERSION: u16 = 4;
const VERSION: u16 = 5;
const CHECKSUM_BYTES: usize = Sha256Digest::LENGTH;

/// Exact nonsensitive native identity retained for safe recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeIdentity {
    process_id: ProcessId,
    preparation_digest: Sha256Digest,
    profile_digest: Sha256Digest,
    helper_digest: Sha256Digest,
    proxy_routing_digest: Option<Sha256Digest>,
    secret_binding_digest: Option<Sha256Digest>,
    root_pid: Option<u32>,
    root_start_token: Option<u64>,
    process_group: Option<u32>,
}

impl RuntimeIdentity {
    /// Creates a recovery-safe identity without secret values, raw routing tokens, or host paths.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "the closed recovery identity keeps each independently matched field explicit"
    )]
    pub const fn new(
        process_id: ProcessId,
        preparation_digest: Sha256Digest,
        profile_digest: Sha256Digest,
        helper_digest: Sha256Digest,
        proxy_routing_digest: Option<Sha256Digest>,
        secret_binding_digest: Option<Sha256Digest>,
        root_pid: Option<u32>,
        process_group: Option<u32>,
    ) -> Self {
        Self {
            process_id,
            preparation_digest,
            profile_digest,
            helper_digest,
            proxy_routing_digest,
            secret_binding_digest,
            root_pid,
            root_start_token: None,
            process_group,
        }
    }

    /// Adds the observed platform birth token for exact PID-reuse protection.
    #[must_use]
    pub const fn with_root_start_token(mut self, root_start_token: Option<u64>) -> Self {
        self.root_start_token = root_start_token;
        self
    }

    /// Returns the C2 process identity.
    #[must_use]
    pub const fn process_id(self) -> ProcessId {
        self.process_id
    }

    /// Returns the admitted preparation identity.
    #[must_use]
    pub const fn preparation_digest(self) -> Sha256Digest {
        self.preparation_digest
    }

    /// Returns the Seatbelt profile identity.
    #[must_use]
    pub const fn profile_digest(self) -> Sha256Digest {
        self.profile_digest
    }

    /// Returns the reviewed helper identity.
    #[must_use]
    pub const fn helper_digest(self) -> Sha256Digest {
        self.helper_digest
    }

    /// Returns the digest of an opaque proxy route identity, never the token itself.
    #[must_use]
    pub const fn proxy_routing_digest(self) -> Option<Sha256Digest> {
        self.proxy_routing_digest
    }

    /// Returns the exact secret-reference/destination digest, never material or handles.
    #[must_use]
    pub const fn secret_binding_digest(self) -> Option<Sha256Digest> {
        self.secret_binding_digest
    }

    /// Returns the observed root PID when activated.
    #[must_use]
    pub const fn root_pid(self) -> Option<u32> {
        self.root_pid
    }

    /// Returns the platform birth token bound to the observed helper root.
    #[must_use]
    pub const fn root_start_token(self) -> Option<u64> {
        self.root_start_token
    }

    /// Returns the C2-owned process group when activated.
    #[must_use]
    pub const fn process_group(self) -> Option<u32> {
        self.process_group
    }

    pub(crate) const fn spawned(self, tree: peritus_process::ProcessTreeIdentity) -> Self {
        Self {
            root_pid: Some(tree.root_pid()),
            root_start_token: tree.start_token(),
            process_group: tree.process_group(),
            ..self
        }
    }
}

/// Independently inspectable ownership state for one backend resource family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryResourceState {
    /// This session never selected the resource family.
    NotRequired,
    /// The original session owner still holds the live resource.
    Live,
    /// The original owner retains exact terminal evidence for cleanup reconciliation.
    CleanupRequired,
    /// Release was completed and durably recorded.
    Released,
    /// A legacy or detached record cannot prove current custody or release.
    Unavailable,
}

/// Exact retained-owner binding and live resource custody for recovery adoption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionCustody {
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
    launch: RecoveryResourceState,
    execution_status: RecoveryResourceState,
    proxy: RecoveryResourceState,
    secrets: RecoveryResourceState,
    resource_monitor: RecoveryResourceState,
}

impl SessionCustody {
    const fn prepared(
        retained_owner: Option<RetainedOwnerBinding>,
        has_proxy: bool,
        has_secrets: bool,
    ) -> Self {
        let (owner_operation_digest, service_owner_digest) = match retained_owner {
            Some(binding) => (
                Some(binding.operation_digest()),
                Some(binding.service_owner().digest()),
            ),
            None => (None, None),
        };
        Self {
            owner_operation_digest,
            service_owner_digest,
            launch: RecoveryResourceState::Live,
            execution_status: RecoveryResourceState::Live,
            proxy: if has_proxy {
                RecoveryResourceState::Live
            } else {
                RecoveryResourceState::NotRequired
            },
            secrets: if has_secrets {
                RecoveryResourceState::Live
            } else {
                RecoveryResourceState::NotRequired
            },
            resource_monitor: RecoveryResourceState::Live,
        }
    }

    const fn legacy(cleanup: CleanupProgress) -> Self {
        let native = if cleanup.helper_quiescent() && cleanup.profile_released() {
            RecoveryResourceState::Released
        } else {
            RecoveryResourceState::Unavailable
        };
        Self {
            owner_operation_digest: None,
            service_owner_digest: None,
            launch: native,
            execution_status: if cleanup.is_complete() {
                RecoveryResourceState::Released
            } else {
                RecoveryResourceState::Unavailable
            },
            proxy: if cleanup.proxy_released() {
                RecoveryResourceState::Released
            } else {
                RecoveryResourceState::Unavailable
            },
            secrets: if cleanup.secrets_released() {
                RecoveryResourceState::Released
            } else {
                RecoveryResourceState::Unavailable
            },
            resource_monitor: native,
        }
    }

    /// Returns the retained operation digest, when the live service owner is authoritative.
    #[must_use]
    pub const fn owner_operation_digest(self) -> Option<Sha256Digest> {
        self.owner_operation_digest
    }

    /// Returns the authenticated service-owner generation digest.
    #[must_use]
    pub const fn service_owner_digest(self) -> Option<Sha256Digest> {
        self.service_owner_digest
    }

    /// Returns protected launch-handle custody.
    #[must_use]
    pub const fn launch(self) -> RecoveryResourceState { self.launch }
    /// Returns execution-status channel and monitor custody.
    #[must_use]
    pub const fn execution_status(self) -> RecoveryResourceState { self.execution_status }
    /// Returns managed-proxy owner custody.
    #[must_use]
    pub const fn proxy(self) -> RecoveryResourceState { self.proxy }
    /// Returns secret leases, material, and delivery-artifact custody.
    #[must_use]
    pub const fn secrets(self) -> RecoveryResourceState { self.secrets }
    /// Returns backend resource-monitor custody.
    #[must_use]
    pub const fn resource_monitor(self) -> RecoveryResourceState { self.resource_monitor }

    pub(crate) const fn adoptable(self, phase: SessionPhase) -> bool {
        if self.owner_operation_digest.is_none()
            || self.service_owner_digest.is_none()
            || phase == SessionPhase::Released
        {
            return false;
        }
        let launch_exact = match phase {
            SessionPhase::Prepared => matches!(self.launch, RecoveryResourceState::Live),
            SessionPhase::Active | SessionPhase::Cancelling | SessionPhase::Terminated => {
                if matches!(self.proxy, RecoveryResourceState::NotRequired)
                    && matches!(self.secrets, RecoveryResourceState::NotRequired)
                {
                    matches!(self.launch, RecoveryResourceState::Released)
                } else {
                    matches!(self.launch, RecoveryResourceState::Live)
                }
            }
            SessionPhase::Released => false,
        };
        launch_exact
            && matches!(self.execution_status, RecoveryResourceState::Live)
            && matches!(
                self.proxy,
                RecoveryResourceState::Live | RecoveryResourceState::NotRequired
            )
            && matches!(
                self.secrets,
                RecoveryResourceState::Live | RecoveryResourceState::NotRequired
            )
            && matches!(self.resource_monitor, RecoveryResourceState::Live)
    }

    const fn released(self) -> bool {
        matches!(self.launch, RecoveryResourceState::Released)
            && matches!(self.execution_status, RecoveryResourceState::Released)
            && matches!(
                self.proxy,
                RecoveryResourceState::Released | RecoveryResourceState::NotRequired
            )
            && matches!(
                self.secrets,
                RecoveryResourceState::Released | RecoveryResourceState::NotRequired
            )
            && matches!(self.resource_monitor, RecoveryResourceState::Released)
    }

    const fn release_execution_status(&mut self) {
        self.execution_status = RecoveryResourceState::Released;
    }

    const fn activated(&mut self) {
        if matches!(self.proxy, RecoveryResourceState::NotRequired)
            && matches!(self.secrets, RecoveryResourceState::NotRequired)
        {
            self.launch = RecoveryResourceState::Released;
        }
    }

    const fn lose_execution_status(&mut self) {
        self.execution_status = RecoveryResourceState::Unavailable;
    }

    const fn release_secrets(&mut self) {
        if !matches!(self.secrets, RecoveryResourceState::NotRequired) {
            self.secrets = RecoveryResourceState::Released;
        }
    }

    const fn release_proxy(&mut self) {
        if !matches!(self.proxy, RecoveryResourceState::NotRequired) {
            self.proxy = RecoveryResourceState::Released;
        }
    }

    const fn require_proxy_cleanup(&mut self) {
        if matches!(self.proxy, RecoveryResourceState::Live) {
            self.proxy = RecoveryResourceState::CleanupRequired;
        }
    }

    const fn release_native(&mut self) {
        self.launch = RecoveryResourceState::Released;
        self.resource_monitor = RecoveryResourceState::Released;
    }
}

/// Monotonic cleanup evidence for every backend-owned resource family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independent cleanup resource family remains explicit"
)]
pub struct CleanupProgress {
    helper_quiescent: bool,
    profile_released: bool,
    proxy_released: bool,
    secrets_released: bool,
    support_threads_joined: bool,
}

impl CleanupProgress {
    /// Returns a newly prepared cleanup record.
    #[must_use]
    pub const fn prepared(has_proxy: bool, has_secrets: bool) -> Self {
        Self {
            helper_quiescent: false,
            profile_released: false,
            proxy_released: !has_proxy,
            secrets_released: !has_secrets,
            support_threads_joined: true,
        }
    }

    /// Returns explicit cleanup facts, used during recovery reconstruction.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "recovery decoder preserves the closed version-one field order"
    )]
    pub const fn from_facts(
        helper_quiescent: bool,
        profile_released: bool,
        proxy_released: bool,
        secrets_released: bool,
        support_threads_joined: bool,
    ) -> Self {
        Self {
            helper_quiescent,
            profile_released,
            proxy_released,
            secrets_released,
            support_threads_joined,
        }
    }

    /// Reports complete teardown of every owned resource family.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        crate::verified::teardown_complete(crate::verified::TeardownFacts {
            helper_quiescent: self.helper_quiescent,
            profile_released: self.profile_released,
            proxy_released: self.proxy_released,
            secrets_released: self.secrets_released,
            support_threads_joined: self.support_threads_joined,
        })
    }

    /// Reports helper-tree quiescence.
    #[must_use]
    pub const fn helper_quiescent(self) -> bool {
        self.helper_quiescent
    }

    /// Reports profile teardown.
    #[must_use]
    pub const fn profile_released(self) -> bool {
        self.profile_released
    }

    /// Reports proxy lease teardown.
    #[must_use]
    pub const fn proxy_released(self) -> bool {
        self.proxy_released
    }

    /// Reports secret lease teardown.
    #[must_use]
    pub const fn secrets_released(self) -> bool {
        self.secrets_released
    }

    /// Reports complete support-thread joins.
    #[must_use]
    pub const fn support_threads_joined(self) -> bool {
        self.support_threads_joined
    }

    pub(crate) const fn mark_native_released(&mut self) {
        self.helper_quiescent = true;
        self.profile_released = true;
    }

    pub(crate) const fn mark_proxy_released(&mut self) {
        self.proxy_released = true;
    }

    pub(crate) const fn mark_secrets_released(&mut self) {
        self.secrets_released = true;
    }

    pub(crate) const fn mark_support_started(&mut self) {
        self.support_threads_joined = false;
    }

    pub(crate) const fn mark_support_joined(&mut self) {
        self.support_threads_joined = true;
    }
}

/// Durable checksummed state supporting exact reopen and cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacosRecoveryRecord {
    identity: RuntimeIdentity,
    activated: bool,
    phase: SessionPhase,
    cancellation: Option<CancellationReason>,
    termination: Option<TerminationReason>,
    custody: SessionCustody,
    cleanup: CleanupProgress,
    materialized_secret_files: Vec<String>,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl MacosRecoveryRecord {
    /// Creates and canonicalizes a current-version runtime record.
    ///
    /// # Errors
    /// Returns a bounded encoding failure.
    pub fn new(
        identity: RuntimeIdentity,
        activated: bool,
        cleanup: CleanupProgress,
    ) -> Result<Self, MacosError> {
        Self::new_with_materialized_secret_files(
            identity,
            activated,
            cleanup,
            Vec::new(),
            None,
        )
    }

    pub(crate) fn new_with_materialized_secret_files(
        identity: RuntimeIdentity,
        activated: bool,
        cleanup: CleanupProgress,
        materialized_secret_files: Vec<String>,
        retained_owner: Option<RetainedOwnerBinding>,
    ) -> Result<Self, MacosError> {
        if retained_owner.is_some_and(|binding| binding.process_id() != identity.process_id())
            || activated
                && !matches!(
                    (
                        identity.root_pid(),
                        identity.root_start_token(),
                        identity.process_group(),
                    ),
                    (Some(root), Some(_), Some(group)) if root == group
                )
        {
            return Err(recovery_state_error(
                "native recovery owner or birth identity differs",
            ));
        }
        let phase = if cleanup.is_complete() && materialized_secret_files.is_empty() {
            SessionPhase::Released
        } else if activated {
            SessionPhase::Active
        } else {
            SessionPhase::Prepared
        };
        let mut custody = SessionCustody::prepared(
            retained_owner,
            identity.proxy_routing_digest().is_some(),
            identity.secret_binding_digest().is_some(),
        );
        if phase == SessionPhase::Active {
            custody.activated();
        } else if phase == SessionPhase::Released {
            custody.release_execution_status();
            custody.release_secrets();
            custody.release_proxy();
            custody.release_native();
        }
        let mut record = Self {
            identity,
            activated,
            phase,
            cancellation: None,
            termination: None,
            custody,
            cleanup,
            materialized_secret_files,
            canonical: Vec::new(),
            digest: Sha256Digest::new([0; 32]),
        };
        record.refresh()?;
        Ok(record)
    }

    /// Returns exact runtime identity.
    #[must_use]
    pub const fn identity(&self) -> RuntimeIdentity {
        self.identity
    }

    /// Reports whether activation was observed.
    #[must_use]
    pub const fn activated(&self) -> bool {
        self.activated
    }

    /// Returns the exact backend lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> SessionPhase { self.phase }

    /// Returns the immutable first accepted cancellation reason.
    #[must_use]
    pub const fn cancellation(&self) -> Option<CancellationReason> { self.cancellation }

    /// Returns the observed terminal category without implying cleanup.
    #[must_use]
    pub const fn termination(&self) -> Option<TerminationReason> { self.termination }

    /// Returns explicit owner and per-resource custody.
    #[must_use]
    pub const fn custody(&self) -> SessionCustody { self.custody }

    /// Returns monotonic cleanup progress.
    #[must_use]
    pub const fn cleanup(&self) -> CleanupProgress {
        self.cleanup
    }

    /// Returns exact file-secret destinations still requiring idempotent removal.
    #[must_use]
    pub fn materialized_secret_files(&self) -> &[String] {
        &self.materialized_secret_files
    }

    /// Returns checksummed canonical record bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Returns the digest of the complete checksummed record.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Classifies current native evidence without acting on mismatched identities.
    #[must_use]
    pub fn classify(
        &self,
        observed: Option<RuntimeIdentity>,
        inspection_accessible: bool,
    ) -> RecoveryClassification {
        if !inspection_accessible {
            return RecoveryClassification::Indeterminate;
        }
        let cleanup_complete = self.phase == SessionPhase::Released
            && self.cleanup.is_complete()
            && self.materialized_secret_files.is_empty();
        match observed {
            Some(identity) if identity != self.identity => RecoveryClassification::Mismatched,
            Some(_) if cleanup_complete => RecoveryClassification::Mismatched,
            Some(_) => RecoveryClassification::Indeterminate,
            None if cleanup_complete => RecoveryClassification::AbsentClean,
            None => RecoveryClassification::Indeterminate,
        }
    }

    /// Classifies adoption evidence emitted by the original retained process owner.
    ///
    /// A copied PID/group tuple is insufficient. Live ownership requires the exact current
    /// checksummed record, authenticated retained-owner generation, birth token, phase, and
    /// complete resource custody reported through the protected owner transport.
    #[must_use]
    pub fn classify_adoption(
        &self,
        observed: &NativeSessionRecovery,
        inspection_accessible: bool,
    ) -> RecoveryClassification {
        if !inspection_accessible {
            return RecoveryClassification::Indeterminate;
        }
        if observed.platform() != NativePlatform::Macos
            || observed.process_id() != self.identity.process_id()
        {
            return RecoveryClassification::Mismatched;
        }
        let phase_matches = matches!(
            (self.phase, observed.phase()),
            (SessionPhase::Prepared, NativeRecoveryPhase::Prepared)
                | (SessionPhase::Active, NativeRecoveryPhase::Active)
                | (SessionPhase::Cancelling, NativeRecoveryPhase::Cancelling)
                | (SessionPhase::Terminated, NativeRecoveryPhase::Terminated)
                | (SessionPhase::Released, NativeRecoveryPhase::Released)
        );
        let tree_matches = observed.tree_identity().map_or_else(
            || self.identity.root_pid().is_none(),
            |tree| {
                self.identity.root_pid() == Some(tree.root_pid())
                    && self.identity.root_start_token() == tree.start_token()
                    && self.identity.process_group() == tree.process_group()
                    && tree.complete_containment()
            },
        );
        if observed.record_digest() == self.digest
            && observed.record() == self.canonical_bytes()
            && observed.owner_operation_digest() == self.custody.owner_operation_digest()
            && observed.service_owner_digest() == self.custody.service_owner_digest()
            && observed.custody_complete()
            && self.custody.adoptable(self.phase)
            && phase_matches
            && tree_matches
        {
            RecoveryClassification::LiveOwned
        } else {
            RecoveryClassification::Indeterminate
        }
    }

    pub(crate) fn record_spawned(
        &mut self,
        tree: peritus_process::ProcessTreeIdentity,
        cleanup: CleanupProgress,
    ) -> Result<(), MacosError> {
        self.identity = self.identity.spawned(tree);
        self.cleanup = cleanup;
        self.refresh()
    }

    pub(crate) fn record_activation(&mut self) -> Result<(), MacosError> {
        self.activated = true;
        self.phase = SessionPhase::Active;
        self.custody.activated();
        self.refresh()
    }

    pub(crate) fn record_cancellation(
        &mut self,
        reason: CancellationReason,
    ) -> Result<(), MacosError> {
        if self.cancellation.is_some_and(|recorded| recorded != reason) {
            return Err(recovery_state_error(
                "native recovery cancellation reason cannot be replaced",
            ));
        }
        self.cancellation = Some(reason);
        self.phase = SessionPhase::Cancelling;
        self.refresh()
    }

    pub(crate) fn record_termination(
        &mut self,
        termination: TerminationReason,
    ) -> Result<(), MacosError> {
        self.termination = Some(termination);
        self.phase = SessionPhase::Terminated;
        self.refresh()
    }

    pub(crate) fn record_cleanup(&mut self, cleanup: CleanupProgress) -> Result<(), MacosError> {
        self.cleanup = cleanup;
        self.refresh()
    }

    pub(crate) fn record_execution_status_released(&mut self) -> Result<(), MacosError> {
        self.custody.release_execution_status();
        self.refresh()
    }

    pub(crate) fn record_execution_status_unavailable(&mut self) -> Result<(), MacosError> {
        self.custody.lose_execution_status();
        self.refresh()
    }

    pub(crate) fn record_secrets_released(&mut self) -> Result<(), MacosError> {
        self.custody.release_secrets();
        self.refresh()
    }

    pub(crate) fn record_proxy_cleanup_required(&mut self) -> Result<(), MacosError> {
        let mut next = self.clone();
        next.custody.require_proxy_cleanup();
        next.refresh()?;
        *self = next;
        Ok(())
    }

    pub(crate) fn record_proxy_released(
        &mut self,
        cleanup: CleanupProgress,
    ) -> Result<(), MacosError> {
        let mut next = self.clone();
        next.custody.release_proxy();
        next.cleanup = cleanup;
        next.refresh()?;
        *self = next;
        Ok(())
    }

    pub(crate) fn record_native_released(&mut self) -> Result<(), MacosError> {
        self.custody.release_native();
        self.refresh()
    }

    pub(crate) fn record_released(&mut self) -> Result<(), MacosError> {
        if !self.cleanup.is_complete()
            || !self.materialized_secret_files.is_empty()
            || !self.custody.released()
        {
            return Err(recovery_state_error(
                "native recovery release lacks complete cleanup evidence",
            ));
        }
        self.phase = SessionPhase::Released;
        self.refresh()
    }

    pub(crate) fn record_materialized_secret_file_released(
        &mut self,
        path: &str,
    ) -> Result<(), MacosError> {
        let Some(index) = self
            .materialized_secret_files
            .iter()
            .position(|candidate| candidate == path)
        else {
            return Ok(());
        };
        let obligation = self.materialized_secret_files.remove(index);
        if let Err(error) = self.refresh() {
            // `remove` retains the vector allocation, so restoring the exact obligation cannot
            // allocate and the previously durable canonical record remains authoritative.
            self.materialized_secret_files.insert(index, obligation);
            return Err(error);
        }
        Ok(())
    }
}

fn recovery_state_error(detail: &'static str) -> MacosError {
    MacosError::new(
        crate::MacosErrorKind::RecoveryIndeterminate,
        crate::MacosOperation::Recover,
        crate::RecoveryAction::Quarantine,
        detail,
    )
}

/// Result of exact native resource classification during recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryClassification {
    /// The original retained owner proved exact live custody and remains attached for supervision.
    LiveOwned,
    /// No resource remains and the record proves complete cleanup or pre-activation absence.
    AbsentClean,
    /// A native identity is present but does not exactly match; it must not be signalled.
    Mismatched,
    /// Inspection, identity reuse, or cleanup ambiguity prevents a safe claim.
    Indeterminate,
}
