//! Version-one durable runtime identity and cleanup records.

use peritus_types::{ProcessId, Sha256Digest};

use crate::MacosError;

mod codec;

const MAGIC: [u8; 8] = *b"PRTSMRC1";
const VERSION: u16 = 1;
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
            process_group,
        }
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

    /// Returns the C2-owned process group when activated.
    #[must_use]
    pub const fn process_group(self) -> Option<u32> {
        self.process_group
    }

    pub(crate) const fn activated(self, root_pid: u32, process_group: Option<u32>) -> Self {
        Self { root_pid: Some(root_pid), process_group, ..self }
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
}

/// Durable checksummed state supporting exact reopen and cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacosRecoveryRecord {
    identity: RuntimeIdentity,
    activated: bool,
    cleanup: CleanupProgress,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl MacosRecoveryRecord {
    /// Creates and canonicalizes a version-one runtime record.
    ///
    /// # Errors
    /// Returns a bounded encoding failure.
    pub fn new(
        identity: RuntimeIdentity,
        activated: bool,
        cleanup: CleanupProgress,
    ) -> Result<Self, MacosError> {
        let mut record = Self {
            identity,
            activated,
            cleanup,
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

    /// Returns monotonic cleanup progress.
    #[must_use]
    pub const fn cleanup(&self) -> CleanupProgress {
        self.cleanup
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
        match observed {
            Some(identity) if identity == self.identity && !self.cleanup.is_complete() => {
                RecoveryClassification::LiveOwned
            }
            Some(identity) if identity != self.identity => RecoveryClassification::Mismatched,
            Some(_) if self.cleanup.is_complete() => RecoveryClassification::Mismatched,
            None if self.cleanup.is_complete() => RecoveryClassification::AbsentClean,
            None if self.activated => RecoveryClassification::Indeterminate,
            None => RecoveryClassification::AbsentClean,
            Some(_) => RecoveryClassification::Indeterminate,
        }
    }

    pub(crate) fn record_activation(
        &mut self,
        root_pid: u32,
        process_group: Option<u32>,
    ) -> Result<(), MacosError> {
        self.identity = self.identity.activated(root_pid, process_group);
        self.activated = true;
        self.refresh()
    }

    pub(crate) fn record_cleanup(&mut self, cleanup: CleanupProgress) -> Result<(), MacosError> {
        self.cleanup = cleanup;
        self.refresh()
    }
}

/// Result of exact native resource classification during recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryClassification {
    /// The exact recorded native identity is still live and may be terminated or cleaned.
    LiveOwned,
    /// No resource remains and the record proves complete cleanup or pre-activation absence.
    AbsentClean,
    /// A native identity is present but does not exactly match; it must not be signalled.
    Mismatched,
    /// Inspection, identity reuse, or cleanup ambiguity prevents a safe claim.
    Indeterminate,
}
