//! Versioned native recovery record and exact ownership classification.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits, decode_frame, encode_frame};
use peritus_process::{NativeWindowsContainmentIdentity, ProcessTreeIdentity};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    WindowsError, WindowsErrorKind, WindowsOperation, WindowsPath, WindowsPhase, WindowsRecovery,
};

const FAMILY: u16 = 0xC317;
const SCHEMA_V1: u16 = 1;
const SCHEMA_V2: u16 = 2;
const SCHEMA_V3: u16 = 3;
const SCHEMA_V4: u16 = 4;
const SCHEMA_V5: u16 = 5;
const SCHEMA_V6: u16 = 6;
const CHECKSUM_BYTES: usize = Sha256Digest::LENGTH;
const LIMITS: CodecLimits = CodecLimits::PRODUCTION;

/// Durable exact identity of one helper-created private secret file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SecretFileRecovery {
    path: WindowsPath,
    payload_len: u64,
    volume_serial: u64,
    file_id: [u8; 16],
}

impl SecretFileRecovery {
    #[cfg(target_os = "windows")]
    pub(crate) fn new(
        path: WindowsPath,
        identity: peritus_process::NativeWindowsSecretFileIdentity,
    ) -> Result<Self, WindowsError> {
        let binding = identity.binding();
        if path.digest() != binding.path_digest() {
            return Err(recovery_error(
                "private secret recovery path differs from helper custody",
            ));
        }
        Self::from_parts(
            path,
            binding.payload_len(),
            identity.volume_serial(),
            identity.file_id(),
        )
    }

    fn from_parts(
        path: WindowsPath,
        payload_len: u64,
        volume_serial: u64,
        file_id: [u8; 16],
    ) -> Result<Self, WindowsError> {
        if payload_len == 0 || volume_serial == 0 || file_id == [0; 16] {
            return Err(recovery_error("private secret recovery identity is incomplete"));
        }
        Ok(Self { path, payload_len, volume_serial, file_id })
    }

    pub(crate) const fn path(&self) -> &WindowsPath {
        &self.path
    }

    pub(crate) const fn payload_len(&self) -> u64 {
        self.payload_len
    }

    pub(crate) const fn volume_serial(&self) -> u64 {
        self.volume_serial
    }

    pub(crate) const fn file_id(&self) -> [u8; 16] {
        self.file_id
    }
}

/// Monotonic cleanup evidence for every independently released Windows resource family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independently progressing cleanup dimension remains canonical"
)]
pub struct RecoveryCleanup {
    job_closed: bool,
    helper_reaped: bool,
    acl_restored: bool,
    secret_files_removed: bool,
    secret_delivery_released: bool,
    handles_closed: bool,
    proxy_joined: bool,
    network_filter_removed: bool,
}

impl RecoveryCleanup {
    pub(crate) const fn prepared(
        job_required: bool,
        acl_required: bool,
        secret_files_required: bool,
        secret_delivery_required: bool,
        handles_required: bool,
        proxy_required: bool,
        network_filter_required: bool,
    ) -> Self {
        Self {
            job_closed: !job_required,
            helper_reaped: false,
            acl_restored: !acl_required,
            secret_files_removed: !secret_files_required,
            secret_delivery_released: !secret_delivery_required,
            handles_closed: !handles_required && !job_required,
            proxy_joined: !proxy_required,
            network_filter_removed: !network_filter_required,
        }
    }

    const fn legacy(acl_restored: bool, secret_files_removed: bool, helper_reaped: bool) -> Self {
        let complete = acl_restored && secret_files_removed && helper_reaped;
        Self {
            job_closed: complete,
            helper_reaped,
            acl_restored,
            secret_files_removed,
            secret_delivery_released: complete,
            handles_closed: complete,
            proxy_joined: complete,
            network_filter_removed: complete,
        }
    }

    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "the schema-five decoder preserves every closed cleanup field explicitly"
    )]
    const fn from_facts(
        job_closed: bool,
        helper_reaped: bool,
        acl_restored: bool,
        secret_files_removed: bool,
        secret_delivery_released: bool,
        handles_closed: bool,
        proxy_joined: bool,
        network_filter_removed: bool,
    ) -> Self {
        Self {
            job_closed,
            helper_reaped,
            acl_restored,
            secret_files_removed,
            secret_delivery_released,
            handles_closed,
            proxy_joined,
            network_filter_removed,
        }
    }

    /// Reports exact Job Object handle closure.
    #[must_use]
    pub const fn job_closed(self) -> bool {
        self.job_closed
    }

    /// Reports helper reap and post-activation worker quiescence.
    #[must_use]
    pub const fn helper_reaped(self) -> bool {
        self.helper_reaped
    }

    /// Reports durable ACL reversal completion.
    #[must_use]
    pub const fn acl_restored(self) -> bool {
        self.acl_restored
    }

    /// Reports exact private secret-file removal.
    #[must_use]
    pub const fn secret_files_removed(self) -> bool {
        self.secret_files_removed
    }

    /// Reports secret lease, material, and staging release.
    #[must_use]
    pub const fn secret_delivery_released(self) -> bool {
        self.secret_delivery_released
    }

    /// Reports closure of every remaining protected inherited handle.
    #[must_use]
    pub const fn handles_closed(self) -> bool {
        self.handles_closed
    }

    /// Reports managed proxy worker reconciliation.
    #[must_use]
    pub const fn proxy_joined(self) -> bool {
        self.proxy_joined
    }

    /// Reports removal of the session-owned dynamic WFP policy.
    #[must_use]
    pub const fn network_filter_removed(self) -> bool {
        self.network_filter_removed
    }

    /// Reports complete teardown without discarding the identities that authorized it.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        crate::verified::teardown_complete(
            self.job_closed,
            self.helper_reaped,
            self.acl_restored,
            self.secret_files_removed,
            self.secret_delivery_released,
            self.handles_closed,
            self.proxy_joined,
            self.network_filter_removed,
        )
    }

    const fn with_legacy_facts(
        mut self,
        acl_restored: bool,
        secret_files_removed: bool,
        helper_reaped: bool,
    ) -> Self {
        self.acl_restored = acl_restored;
        self.secret_files_removed = secret_files_removed;
        self.helper_reaped = helper_reaped;
        self
    }

    const fn regresses_to(self, next: Self) -> bool {
        self.job_closed && !next.job_closed
            || self.helper_reaped && !next.helper_reaped
            || self.acl_restored && !next.acl_restored
            || self.secret_files_removed && !next.secret_files_removed
            || self.secret_delivery_released && !next.secret_delivery_released
            || self.handles_closed && !next.handles_closed
            || self.proxy_joined && !next.proxy_joined
            || self.network_filter_removed && !next.network_filter_removed
    }

    const fn mark(&mut self, dimension: RecoveryCleanupDimension) {
        match dimension {
            RecoveryCleanupDimension::Job => self.job_closed = true,
            RecoveryCleanupDimension::Helper => self.helper_reaped = true,
            RecoveryCleanupDimension::Acl => self.acl_restored = true,
            RecoveryCleanupDimension::SecretFiles => self.secret_files_removed = true,
            RecoveryCleanupDimension::SecretDelivery => self.secret_delivery_released = true,
            RecoveryCleanupDimension::Handles => self.handles_closed = true,
            RecoveryCleanupDimension::Proxy => self.proxy_joined = true,
            RecoveryCleanupDimension::NetworkFilter => self.network_filter_removed = true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryCleanupDimension {
    Job,
    Helper,
    Acl,
    SecretFiles,
    SecretDelivery,
    Handles,
    Proxy,
    NetworkFilter,
}

/// Nonsensitive identities for resources owned by one native session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeIdentity {
    process_id: ProcessId,
    preparation_digest: Sha256Digest,
    helper_digest: Sha256Digest,
    job_identity: Sha256Digest,
    profile_identity: Sha256Digest,
    acl_digest: Sha256Digest,
}

impl RuntimeIdentity {
    /// Creates complete native identity without raw handles or private paths.
    #[must_use]
    pub const fn new(
        process_id: ProcessId,
        preparation_digest: Sha256Digest,
        helper_digest: Sha256Digest,
        job_identity: Sha256Digest,
        profile_identity: Sha256Digest,
        acl_digest: Sha256Digest,
    ) -> Self {
        Self {
            process_id,
            preparation_digest,
            helper_digest,
            job_identity,
            profile_identity,
            acl_digest,
        }
    }

    /// Returns owning C2 process.
    #[must_use]
    pub const fn process_id(self) -> ProcessId {
        self.process_id
    }
    /// Returns preparation identity.
    #[must_use]
    pub const fn preparation_digest(self) -> Sha256Digest {
        self.preparation_digest
    }
    /// Returns helper identity.
    #[must_use]
    pub const fn helper_digest(self) -> Sha256Digest {
        self.helper_digest
    }
    /// Returns Job Object identity digest.
    #[must_use]
    pub const fn job_identity(self) -> Sha256Digest {
        self.job_identity
    }
    /// Returns token/AppContainer identity digest.
    #[must_use]
    pub const fn profile_identity(self) -> Sha256Digest {
        self.profile_identity
    }
    /// Returns temporary ACL plan digest.
    #[must_use]
    pub const fn acl_digest(self) -> Sha256Digest {
        self.acl_digest
    }
}

/// Stable identities of the independently retained owners needed for adoption and cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsOwnerIdentity {
    job: Option<Sha256Digest>,
    helper_channels: Option<Sha256Digest>,
    protected_handles: Option<Sha256Digest>,
    proxy: Option<Sha256Digest>,
    network_filter: Option<Sha256Digest>,
    secret_delivery: Option<Sha256Digest>,
    acl_reversal: Option<Sha256Digest>,
    dispatch: Sha256Digest,
}

impl WindowsOwnerIdentity {
    #[allow(clippy::too_many_arguments, reason = "each independently owned resource is explicit")]
    pub(crate) const fn new(
        job: Option<Sha256Digest>,
        helper_channels: Option<Sha256Digest>,
        protected_handles: Option<Sha256Digest>,
        proxy: Option<Sha256Digest>,
        network_filter: Option<Sha256Digest>,
        secret_delivery: Option<Sha256Digest>,
        acl_reversal: Option<Sha256Digest>,
        dispatch: Sha256Digest,
    ) -> Self {
        Self {
            job,
            helper_channels,
            protected_handles,
            proxy,
            network_filter,
            secret_delivery,
            acl_reversal,
            dispatch,
        }
    }

    fn valid_for(self, cleanup: RecoveryCleanup, containment_required: bool) -> bool {
        self.dispatch != Sha256Digest::new([0; 32])
            && (cleanup.job_closed() || !containment_required || self.job.is_some())
            && (cleanup.handles_closed()
                || self.helper_channels.is_some() && self.protected_handles.is_some())
            && (cleanup.proxy_joined() || self.proxy.is_some())
            && (cleanup.network_filter_removed() || self.network_filter.is_some())
            && (cleanup.secret_delivery_released() || self.secret_delivery.is_some())
            && (cleanup.acl_restored() || self.acl_reversal.is_some())
    }

    fn selected_for(self, cleanup: RecoveryCleanup) -> Self {
        Self {
            job: (!cleanup.job_closed()).then_some(self.job).flatten(),
            helper_channels: (!cleanup.handles_closed())
                .then_some(self.helper_channels)
                .flatten(),
            protected_handles: (!cleanup.handles_closed())
                .then_some(self.protected_handles)
                .flatten(),
            proxy: (!cleanup.proxy_joined()).then_some(self.proxy).flatten(),
            network_filter: (!cleanup.network_filter_removed())
                .then_some(self.network_filter)
                .flatten(),
            secret_delivery: (!cleanup.secret_delivery_released())
                .then_some(self.secret_delivery)
                .flatten(),
            acl_reversal: (!cleanup.acl_restored())
                .then_some(self.acl_reversal)
                .flatten(),
            dispatch: self.dispatch,
        }
    }

    pub(crate) fn digest(self) -> Sha256Digest {
        let mut bytes = Vec::from(b"PERITUS-WINDOWS-OWNER-IDENTITY-V1\0".as_slice());
        for value in [
            self.job,
            self.helper_channels,
            self.protected_handles,
            self.proxy,
            self.network_filter,
            self.secret_delivery,
            self.acl_reversal,
        ] {
            match value {
                Some(value) => {
                    bytes.push(1);
                    bytes.extend_from_slice(value.as_bytes());
                }
                None => bytes.push(0),
            }
        }
        bytes.extend_from_slice(self.dispatch.as_bytes());
        peritus_codec::sha256(&bytes)
    }
}

/// Durable C3 supporting recovery evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsRecoveryRecord {
    schema: u16,
    identity: RuntimeIdentity,
    phase: WindowsPhase,
    containment_required: bool,
    cleanup: RecoveryCleanup,
    tree: Option<ProcessTreeIdentity>,
    containment: Option<NativeWindowsContainmentIdentity>,
    secret_files: Vec<SecretFileRecovery>,
    acl_transaction_digest: Option<Sha256Digest>,
    acl_receipt: Option<Sha256Digest>,
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
    owner_identity: Option<WindowsOwnerIdentity>,
    canonical: Vec<u8>,
}

impl WindowsRecoveryRecord {
    /// Creates a prepared recovery record.
    #[must_use]
    pub fn prepared(identity: RuntimeIdentity) -> Self {
        let mut value = Self {
            schema: SCHEMA_V5,
            identity,
            phase: WindowsPhase::Prepared,
            containment_required: false,
            cleanup: RecoveryCleanup::prepared(false, true, true, false, false, false, false),
            tree: None,
            containment: None,
            secret_files: Vec::new(),
            acl_transaction_digest: None,
            acl_receipt: None,
            owner_operation_digest: None,
            service_owner_digest: None,
            owner_identity: None,
            canonical: Vec::new(),
        };
        value.canonical = value.encode().unwrap_or_default();
        value
    }

    pub(crate) fn prepared_owned(
        identity: RuntimeIdentity,
        containment_required: bool,
        cleanup: RecoveryCleanup,
        transaction_digest: Option<Sha256Digest>,
        receipt: Option<Sha256Digest>,
        owner_operation_digest: Option<Sha256Digest>,
        service_owner_digest: Option<Sha256Digest>,
        owner_identity: Option<WindowsOwnerIdentity>,
    ) -> Self {
        let complete_transaction = transaction_digest.is_some() == receipt.is_some();
        let complete_owner = owner_operation_digest.is_some() == service_owner_digest.is_some();
        let mut value = Self {
            schema: if owner_identity.is_some() { SCHEMA_V6 } else { SCHEMA_V5 },
            identity,
            phase: WindowsPhase::Prepared,
            containment_required,
            cleanup,
            tree: None,
            containment: None,
            secret_files: Vec::new(),
            acl_transaction_digest: complete_transaction.then_some(transaction_digest).flatten(),
            acl_receipt: complete_transaction.then_some(receipt).flatten(),
            owner_operation_digest: complete_owner.then_some(owner_operation_digest).flatten(),
            service_owner_digest: complete_owner.then_some(service_owner_digest).flatten(),
            owner_identity,
            canonical: Vec::new(),
        };
        value.canonical = value.encode().unwrap_or_default();
        value
    }

    pub(crate) fn spawned(&mut self, tree: ProcessTreeIdentity) -> Result<(), WindowsError> {
        if tree.root_pid() == 0
            || tree.start_token().is_none()
            || tree.process_group().is_some()
            || !tree.complete_containment()
        {
            return Err(recovery_error("native recovery helper birth identity is incomplete"));
        }
        if self.tree == Some(tree) {
            return Ok(());
        }
        if self.tree.is_some() {
            return Err(recovery_error("native recovery helper birth identity changed"));
        }
        self.tree = Some(tree);
        self.canonical = self.encode()?;
        Ok(())
    }

    pub(crate) fn adopted(
        &mut self,
        containment: NativeWindowsContainmentIdentity,
    ) -> Result<(), WindowsError> {
        if !matches!(self.schema, SCHEMA_V3 | SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6)
            || !self.containment_required
        {
            return Err(recovery_error(
                "legacy Windows recovery record cannot claim live Job Object adoption",
            ));
        }
        let helper = self
            .tree
            .ok_or_else(|| recovery_error("Windows containment preceded helper birth identity"))?;
        if containment.job_identity() != self.identity.job_identity
            || containment.object_name()
                != crate::identity::job_name(self.identity.process_id, self.identity.job_identity)
            || containment.target_identity().root_pid() == helper.root_pid()
        {
            return Err(recovery_error(
                "Windows Job Object or target identity differs from its authority",
            ));
        }
        if self.containment.as_ref() == Some(&containment) {
            return Ok(());
        }
        if self.containment.is_some() {
            return Err(recovery_error("Windows containment adoption identity changed"));
        }
        self.containment = Some(containment);
        self.canonical = self.encode()?;
        Ok(())
    }

    pub(crate) fn retain_secret_files(
        &mut self,
        mut files: Vec<SecretFileRecovery>,
    ) -> Result<(), WindowsError> {
        if !matches!(self.schema, SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6)
            || self.phase != WindowsPhase::Prepared
        {
            return Err(recovery_error(
                "private secret custody cannot be attached to this recovery phase",
            ));
        }
        files.sort_by(|left, right| {
            left.path.digest().as_bytes().cmp(right.path.digest().as_bytes())
        });
        if files
            .windows(2)
            .any(|pair| pair[0].path.digest() == pair[1].path.digest())
        {
            return Err(recovery_error(
                "private secret recovery paths contain a duplicate identity",
            ));
        }
        if self.secret_files == files {
            return Ok(());
        }
        if !self.secret_files.is_empty() {
            return Err(recovery_error("private secret recovery custody changed"));
        }
        self.secret_files = files;
        self.canonical = self.encode()?;
        Ok(())
    }

    /// Advances a record monotonically with cleanup facts.
    ///
    /// # Errors
    /// Rejects lifecycle regression or a released record missing cleanup evidence.
    pub fn advance(
        &mut self,
        phase: WindowsPhase,
        acl_restored: bool,
        secret_files_removed: bool,
        helper_reaped: bool,
    ) -> Result<(), WindowsError> {
        let requested_complete = acl_restored && secret_files_removed && helper_reaped;
        let mut cleanup = self.cleanup.with_legacy_facts(
            acl_restored,
            secret_files_removed,
            helper_reaped,
        );
        if !matches!(self.schema, SCHEMA_V5 | SCHEMA_V6) && requested_complete {
            cleanup = RecoveryCleanup::legacy(
                acl_restored,
                secret_files_removed,
                helper_reaped,
            );
        }
        let legacy_partial = !matches!(self.schema, SCHEMA_V5 | SCHEMA_V6)
            && (acl_restored || secret_files_removed || helper_reaped)
            && !requested_complete;
        if legacy_partial {
            return Err(recovery_error(
                "legacy native recovery schema cannot represent partial cleanup",
            ));
        }
        self.update_progress(phase, cleanup)
    }

    pub(crate) fn advance_phase(&mut self, phase: WindowsPhase) -> Result<(), WindowsError> {
        self.update_progress(phase, self.cleanup)
    }

    pub(crate) fn advance_phase_with_cleanup(
        &mut self,
        phase: WindowsPhase,
        dimension: RecoveryCleanupDimension,
    ) -> Result<(), WindowsError> {
        let mut cleanup = self.cleanup;
        cleanup.mark(dimension);
        self.update_progress(phase, cleanup)
    }

    pub(crate) fn mark_cleanup(
        &mut self,
        dimension: RecoveryCleanupDimension,
    ) -> Result<(), WindowsError> {
        let mut cleanup = self.cleanup;
        cleanup.mark(dimension);
        self.update_progress(self.phase, cleanup)
    }

    /// Records complete abort cleanup without inventing activation or termination phases.
    ///
    /// # Errors
    /// Rejects incomplete cleanup facts or a record already released normally.
    pub fn record_cleanup(
        &mut self,
        acl_restored: bool,
        secret_files_removed: bool,
        helper_reaped: bool,
    ) -> Result<(), WindowsError> {
        if self.phase == WindowsPhase::Released
            || !(acl_restored && secret_files_removed && helper_reaped)
        {
            return Err(recovery_error(
                "abort cleanup facts are incomplete or no longer applicable",
            ));
        }
        let cleanup = if matches!(self.schema, SCHEMA_V5 | SCHEMA_V6) {
            self.cleanup.with_legacy_facts(
                acl_restored,
                secret_files_removed,
                helper_reaped,
            )
        } else {
            RecoveryCleanup::legacy(acl_restored, secret_files_removed, helper_reaped)
        };
        if !cleanup.is_complete() {
            return Err(recovery_error(
                "abort cleanup lacks independently completed resource evidence",
            ));
        }
        self.update_progress(self.phase, cleanup)
    }

    fn update_progress(
        &mut self,
        phase: WindowsPhase,
        cleanup: RecoveryCleanup,
    ) -> Result<(), WindowsError> {
        if !crate::verified::recovery_advance_allowed(self.phase.ordinal(), phase.ordinal())
            || self.cleanup.regresses_to(cleanup)
            || phase == WindowsPhase::Released && !cleanup.is_complete()
            || cleanup.handles_closed() && !cleanup.job_closed()
        {
            return Err(recovery_error("native recovery lifecycle or cleanup facts are invalid"));
        }
        let previous_phase = self.phase;
        let previous_cleanup = self.cleanup;
        self.phase = phase;
        self.cleanup = cleanup;
        match self.encode() {
            Ok(canonical) => {
                self.canonical = canonical;
                Ok(())
            }
            Err(error) => {
                self.phase = previous_phase;
                self.cleanup = previous_cleanup;
                Err(error)
            }
        }
    }

    /// Decodes a checksummed version-one record.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, or inconsistent records.
    pub fn decode(bytes: &[u8]) -> Result<Self, WindowsError> {
        if bytes.len() <= CHECKSUM_BYTES {
            return Err(recovery_error("native recovery record is truncated"));
        }
        let split = bytes.len() - CHECKSUM_BYTES;
        if peritus_codec::sha256(&bytes[..split]).as_bytes() != &bytes[split..] {
            return Err(recovery_error("native recovery checksum mismatched"));
        }
        let frame = decode_frame(&bytes[..split], LIMITS)
            .map_err(|_| recovery_error("native recovery frame is invalid"))?;
        let schema = frame.header().schema_version();
        if frame.header().family() != FAMILY
            || !matches!(
                schema,
                SCHEMA_V1 | SCHEMA_V2 | SCHEMA_V3 | SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6
            )
        {
            return Err(recovery_error("native recovery schema is unsupported"));
        }
        let mut reader = CanonicalReader::new(frame.payload(), LIMITS);
        let identity = RuntimeIdentity::new(
            ProcessId::new(reader.read_fixed().map_err(codec_failure)?)
                .map_err(|_| recovery_error("native recovery process identity is zero"))?,
            read_digest(&mut reader)?,
            read_digest(&mut reader)?,
            read_digest(&mut reader)?,
            read_digest(&mut reader)?,
            read_digest(&mut reader)?,
        );
        let phase = WindowsPhase::from_ordinal(reader.read_u8().map_err(codec_failure)?)
            .ok_or_else(|| recovery_error("native recovery phase is unknown"))?;
        let acl_restored = reader.read_bool().map_err(codec_failure)?;
        let secret_files_removed = reader.read_bool().map_err(codec_failure)?;
        let helper_reaped = reader.read_bool().map_err(codec_failure)?;
        let cleanup = if matches!(schema, SCHEMA_V5 | SCHEMA_V6) {
            RecoveryCleanup::from_facts(
                reader.read_bool().map_err(codec_failure)?,
                helper_reaped,
                acl_restored,
                secret_files_removed,
                reader.read_bool().map_err(codec_failure)?,
                reader.read_bool().map_err(codec_failure)?,
                reader.read_bool().map_err(codec_failure)?,
                reader.read_bool().map_err(codec_failure)?,
            )
        } else {
            RecoveryCleanup::legacy(acl_restored, secret_files_removed, helper_reaped)
        };
        let containment_required = if matches!(schema, SCHEMA_V5 | SCHEMA_V6) {
            reader.read_bool().map_err(codec_failure)?
        } else {
            matches!(schema, SCHEMA_V3 | SCHEMA_V4)
        };
        let (
            tree,
            acl_transaction_digest,
            acl_receipt,
            owner_operation_digest,
            service_owner_digest,
        ) = if matches!(
            schema,
            SCHEMA_V2 | SCHEMA_V3 | SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6
        ) {
            let tree = if reader.read_option_tag().map_err(codec_failure)? {
                let root = reader.read_u32().map_err(codec_failure)?;
                let start = if reader.read_option_tag().map_err(codec_failure)? {
                    Some(reader.read_u64().map_err(codec_failure)?)
                } else {
                    None
                };
                let group = if reader.read_option_tag().map_err(codec_failure)? {
                    Some(reader.read_u32().map_err(codec_failure)?)
                } else {
                    None
                };
                let complete = reader.read_bool().map_err(codec_failure)?;
                let tree = ProcessTreeIdentity::new(root, start, group, complete);
                if root == 0 || start.is_none() || group.is_some() || !complete {
                    return Err(recovery_error("native recovery helper birth identity is invalid"));
                }
                Some(tree)
            } else {
                None
            };
            let (transaction, receipt) = if reader.read_option_tag().map_err(codec_failure)? {
                (Some(read_digest(&mut reader)?), Some(read_digest(&mut reader)?))
            } else {
                (None, None)
            };
            let (owner, service) = if reader.read_option_tag().map_err(codec_failure)? {
                (Some(read_digest(&mut reader)?), Some(read_digest(&mut reader)?))
            } else {
                (None, None)
            };
            (tree, transaction, receipt, owner, service)
        } else {
            (None, None, None, None, None)
        };
        let containment = if matches!(schema, SCHEMA_V3 | SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6)
            && reader.read_option_tag().map_err(codec_failure)?
        {
            let job_identity = read_digest(&mut reader)?;
            let object_name = reader.read_str().map_err(codec_failure)?.to_owned();
            let root = reader.read_u32().map_err(codec_failure)?;
            let start = if reader.read_option_tag().map_err(codec_failure)? {
                Some(reader.read_u64().map_err(codec_failure)?)
            } else {
                None
            };
            let group = if reader.read_option_tag().map_err(codec_failure)? {
                Some(reader.read_u32().map_err(codec_failure)?)
            } else {
                None
            };
            let complete = reader.read_bool().map_err(codec_failure)?;
            let target = ProcessTreeIdentity::new(root, start, group, complete);
            let containment = NativeWindowsContainmentIdentity::new(
                job_identity,
                object_name,
                target,
            )
            .map_err(|_| recovery_error("native recovery containment identity is invalid"))?;
            if containment.job_identity() != identity.job_identity()
                || containment.object_name()
                    != crate::identity::job_name(identity.process_id(), identity.job_identity())
                || tree.is_none_or(|helper| helper.root_pid() == root)
            {
                return Err(recovery_error(
                    "native recovery containment is not authority-bound",
                ));
            }
            Some(containment)
        } else {
            None
        };
        let secret_files = if matches!(schema, SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6) {
            let count = reader
                .read_collection_len(4 + 8 + 8 + 16)
                .map_err(codec_failure)?;
            let mut files = reader.reserve_collection(count).map_err(codec_failure)?;
            for _ in 0..count {
                let path = read_windows_path(&mut reader)?;
                let payload_len = reader.read_u64().map_err(codec_failure)?;
                let volume_serial = reader.read_u64().map_err(codec_failure)?;
                let file_id = reader.read_fixed().map_err(codec_failure)?;
                files.push(SecretFileRecovery::from_parts(
                    path,
                    payload_len,
                    volume_serial,
                    file_id,
                )?);
            }
            if files.windows(2).any(|pair| {
                pair[0].path.digest().as_bytes() >= pair[1].path.digest().as_bytes()
            }) {
                return Err(recovery_error(
                    "private secret recovery paths are duplicated or noncanonical",
                ));
            }
            files
        } else {
            Vec::new()
        };
        let owner_identity = if schema == SCHEMA_V6 {
            let job = read_optional_digest(&mut reader)?;
            let helper_channels = read_optional_digest(&mut reader)?;
            let protected_handles = read_optional_digest(&mut reader)?;
            let proxy = read_optional_digest(&mut reader)?;
            let network_filter = read_optional_digest(&mut reader)?;
            let secret_delivery = read_optional_digest(&mut reader)?;
            let acl_reversal = read_optional_digest(&mut reader)?;
            let dispatch = read_digest(&mut reader)?;
            Some(WindowsOwnerIdentity::new(
                job,
                helper_channels,
                protected_handles,
                proxy,
                network_filter,
                secret_delivery,
                acl_reversal,
                dispatch,
            ))
        } else {
            None
        };
        reader.finish().map_err(codec_failure)?;
        let legacy_any_cleanup = acl_restored || secret_files_removed || helper_reaped;
        let legacy_complete_cleanup = acl_restored && secret_files_removed && helper_reaped;
        if !matches!(schema, SCHEMA_V5 | SCHEMA_V6)
            && (phase == WindowsPhase::Released || legacy_any_cleanup)
            && !legacy_complete_cleanup
            || matches!(schema, SCHEMA_V5 | SCHEMA_V6)
                && (phase == WindowsPhase::Released && !cleanup.is_complete()
                    || cleanup.handles_closed() && !cleanup.job_closed()
                    || !containment_required && !cleanup.job_closed()
                    || !containment_required && containment.is_some())
            || schema == SCHEMA_V6
                && owner_identity.is_none_or(|identity| {
                    !identity.valid_for(cleanup, containment_required)
                })
        {
            return Err(recovery_error("native recovery record has inconsistent cleanup facts"));
        }
        if containment_required
            && matches!(
                phase,
                WindowsPhase::Activated | WindowsPhase::CancelRequested | WindowsPhase::Terminated
            )
            && containment.is_none()
        {
            return Err(recovery_error(
                "native recovery active phase lacks exact Windows containment",
            ));
        }
        let value = Self {
            schema,
            identity,
            phase,
            containment_required,
            cleanup,
            tree,
            containment,
            secret_files,
            acl_transaction_digest,
            acl_receipt,
            owner_operation_digest,
            service_owner_digest,
            owner_identity,
            canonical: bytes.to_vec(),
        };
        if value.encode()? != bytes {
            return Err(recovery_error("native recovery record is noncanonical"));
        }
        Ok(value)
    }

    /// Returns exact native identity.
    #[must_use]
    pub const fn identity(&self) -> RuntimeIdentity {
        self.identity
    }
    /// Returns lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> WindowsPhase {
        self.phase
    }
    /// Returns the exact helper birth identity accepted before protocol delivery.
    #[must_use]
    pub const fn tree_identity(&self) -> Option<ProcessTreeIdentity> {
        self.tree
    }
    /// Returns the exact authority-bound Job Object and target birth identity.
    #[must_use]
    pub const fn containment_identity(&self) -> Option<&NativeWindowsContainmentIdentity> {
        self.containment.as_ref()
    }
    /// Returns exact private secret-file cleanup obligations.
    #[must_use]
    pub(crate) fn secret_files(&self) -> &[SecretFileRecovery] {
        &self.secret_files
    }
    /// Returns the authorization-bound ACL transaction identity, when recorded.
    #[must_use]
    pub const fn acl_transaction_digest(&self) -> Option<Sha256Digest> {
        self.acl_transaction_digest
    }
    /// Returns the exact durable ACL journal receipt, when recorded.
    #[must_use]
    pub const fn acl_receipt(&self) -> Option<Sha256Digest> {
        self.acl_receipt
    }
    /// Returns the retained owner operation bound to this session.
    #[must_use]
    pub const fn owner_operation_digest(&self) -> Option<Sha256Digest> {
        self.owner_operation_digest
    }
    /// Returns the retained service-owner generation bound to this session.
    #[must_use]
    pub const fn service_owner_digest(&self) -> Option<Sha256Digest> {
        self.service_owner_digest
    }
    /// Returns durable canonical bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    /// Returns monotonic cleanup evidence for every owned resource family.
    #[must_use]
    pub const fn cleanup(&self) -> RecoveryCleanup {
        self.cleanup
    }
    /// Reports complete teardown evidence.
    #[must_use]
    pub const fn cleanup_complete(&self) -> bool {
        self.cleanup.is_complete()
    }

    /// Returns the stable identity of every resource retained by the original live owner.
    #[must_use]
    pub fn owner_identity_digest(&self) -> Option<Sha256Digest> {
        self.owner_identity.map(WindowsOwnerIdentity::digest)
    }

    /// Returns the exact identity expected for resources whose cleanup is still outstanding.
    #[must_use]
    pub fn live_custody_digest(&self) -> Option<Sha256Digest> {
        self.owner_identity
            .map(|identity| identity.selected_for(self.cleanup).digest())
    }

    pub(crate) const fn custody_complete(&self) -> bool {
        self.schema == SCHEMA_V6
            && self.owner_operation_digest.is_some()
            && self.service_owner_digest.is_some()
            && self.acl_transaction_digest.is_some()
            && self.acl_receipt.is_some()
            && self.owner_identity.is_some()
            && self.phase != WindowsPhase::Released
            && !self.cleanup_complete()
            && (!self.containment_required
                || self.phase == WindowsPhase::Prepared
                || self.containment.is_some())
    }

    fn encode(&self) -> Result<Vec<u8>, WindowsError> {
        let mut writer = CanonicalWriter::new(LIMITS);
        writer.write_fixed(self.identity.process_id.as_bytes()).map_err(codec_failure)?;
        for digest in [
            self.identity.preparation_digest,
            self.identity.helper_digest,
            self.identity.job_identity,
            self.identity.profile_identity,
            self.identity.acl_digest,
        ] {
            writer.write_fixed(digest.as_bytes()).map_err(codec_failure)?;
        }
        writer.write_u8(self.phase.ordinal()).map_err(codec_failure)?;
        writer.write_bool(self.cleanup.acl_restored()).map_err(codec_failure)?;
        writer.write_bool(self.cleanup.secret_files_removed()).map_err(codec_failure)?;
        writer.write_bool(self.cleanup.helper_reaped()).map_err(codec_failure)?;
        if matches!(self.schema, SCHEMA_V5 | SCHEMA_V6) {
            writer.write_bool(self.cleanup.job_closed()).map_err(codec_failure)?;
            writer
                .write_bool(self.cleanup.secret_delivery_released())
                .map_err(codec_failure)?;
            writer.write_bool(self.cleanup.handles_closed()).map_err(codec_failure)?;
            writer.write_bool(self.cleanup.proxy_joined()).map_err(codec_failure)?;
            writer
                .write_bool(self.cleanup.network_filter_removed())
                .map_err(codec_failure)?;
            writer.write_bool(self.containment_required).map_err(codec_failure)?;
        }
        if matches!(
            self.schema,
            SCHEMA_V2 | SCHEMA_V3 | SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6
        ) {
            writer.write_option_tag(self.tree.is_some()).map_err(codec_failure)?;
            if let Some(tree) = self.tree {
                writer.write_u32(tree.root_pid()).map_err(codec_failure)?;
                writer.write_option_tag(tree.start_token().is_some()).map_err(codec_failure)?;
                if let Some(start) = tree.start_token() {
                    writer.write_u64(start).map_err(codec_failure)?;
                }
                writer.write_option_tag(tree.process_group().is_some()).map_err(codec_failure)?;
                if let Some(group) = tree.process_group() {
                    writer.write_u32(group).map_err(codec_failure)?;
                }
                writer.write_bool(tree.complete_containment()).map_err(codec_failure)?;
            }
            let acl_bound = self.acl_transaction_digest.is_some() && self.acl_receipt.is_some();
            writer.write_option_tag(acl_bound).map_err(codec_failure)?;
            if acl_bound {
                writer
                    .write_fixed(self.acl_transaction_digest.expect("checked").as_bytes())
                    .map_err(codec_failure)?;
                writer
                    .write_fixed(self.acl_receipt.expect("checked").as_bytes())
                    .map_err(codec_failure)?;
            }
            let owner_bound = self.owner_operation_digest.is_some()
                && self.service_owner_digest.is_some();
            writer.write_option_tag(owner_bound).map_err(codec_failure)?;
            if owner_bound {
                writer
                    .write_fixed(self.owner_operation_digest.expect("checked").as_bytes())
                    .map_err(codec_failure)?;
                writer
                    .write_fixed(self.service_owner_digest.expect("checked").as_bytes())
                    .map_err(codec_failure)?;
            }
            if matches!(
                self.schema,
                SCHEMA_V3 | SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6
            ) {
                writer.write_option_tag(self.containment.is_some()).map_err(codec_failure)?;
                if let Some(containment) = &self.containment {
                    writer
                        .write_fixed(containment.job_identity().as_bytes())
                        .map_err(codec_failure)?;
                    writer.write_str(containment.object_name()).map_err(codec_failure)?;
                    let target = containment.target_identity();
                    writer.write_u32(target.root_pid()).map_err(codec_failure)?;
                    writer
                        .write_option_tag(target.start_token().is_some())
                        .map_err(codec_failure)?;
                    if let Some(start) = target.start_token() {
                        writer.write_u64(start).map_err(codec_failure)?;
                    }
                    writer
                        .write_option_tag(target.process_group().is_some())
                        .map_err(codec_failure)?;
                    if let Some(group) = target.process_group() {
                        writer.write_u32(group).map_err(codec_failure)?;
                    }
                    writer
                        .write_bool(target.complete_containment())
                        .map_err(codec_failure)?;
                }
            }
            if matches!(self.schema, SCHEMA_V4 | SCHEMA_V5 | SCHEMA_V6) {
                writer
                    .write_collection_len(self.secret_files.len())
                    .map_err(codec_failure)?;
                for file in &self.secret_files {
                    write_windows_path(&mut writer, &file.path)?;
                    writer.write_u64(file.payload_len).map_err(codec_failure)?;
                    writer.write_u64(file.volume_serial).map_err(codec_failure)?;
                    writer.write_fixed(&file.file_id).map_err(codec_failure)?;
                }
            }
        }
        if self.schema == SCHEMA_V6 {
            let owner = self.owner_identity.ok_or_else(|| {
                recovery_error("native recovery owner identity is missing")
            })?;
            for value in [
                owner.job,
                owner.helper_channels,
                owner.protected_handles,
                owner.proxy,
                owner.network_filter,
                owner.secret_delivery,
                owner.acl_reversal,
            ] {
                writer.write_option_tag(value.is_some()).map_err(codec_failure)?;
                if let Some(value) = value {
                    writer.write_fixed(value.as_bytes()).map_err(codec_failure)?;
                }
            }
            writer.write_fixed(owner.dispatch.as_bytes()).map_err(codec_failure)?;
        }
        let mut bytes =
            encode_frame(FAMILY, self.schema, &writer.into_bytes(), LIMITS).map_err(codec_failure)?;
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        Ok(bytes)
    }
}

/// Runtime observation used to classify one durable record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryProbe {
    /// Every native identity is live and exactly matches.
    LiveOwned(RuntimeIdentity),
    /// Every outstanding owner family was inspected and produced this exact custody digest.
    LiveInspected(RuntimeIdentity, Sha256Digest),
    /// No named native resource remains.
    Absent,
    /// A resource exists under a different identity.
    Mismatched,
    /// The operating system cannot establish identity.
    Indeterminate,
}

/// Exact recovery classification; only `LiveOwned` permits cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryClassification {
    /// Exact live resources may be cancelled and cleaned.
    LiveOwned,
    /// Durable released record and absent resources prove clean state.
    AbsentClean,
    /// Identity reuse or drift forbids cleanup.
    Mismatched,
    /// Missing/inaccessible evidence blocks quiescence.
    Indeterminate,
}

/// Classifies native state without guessing ownership.
#[must_use]
pub fn classify(
    record: Option<&WindowsRecoveryRecord>,
    probe: RecoveryProbe,
) -> RecoveryClassification {
    match (record, probe) {
        (Some(record), RecoveryProbe::LiveInspected(identity, custody))
            if record.identity == identity
                && record.custody_complete()
                && record.live_custody_digest() == Some(custody) =>
        {
            RecoveryClassification::LiveOwned
        }
        (Some(record), RecoveryProbe::Absent) if record.cleanup_complete() => {
            RecoveryClassification::AbsentClean
        }
        (Some(record), RecoveryProbe::LiveOwned(identity)) if record.identity != identity => {
            RecoveryClassification::Mismatched
        }
        (Some(_), RecoveryProbe::LiveInspected(_, _) | RecoveryProbe::Mismatched) => {
            RecoveryClassification::Mismatched
        }
        _ => RecoveryClassification::Indeterminate,
    }
}

fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, WindowsError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec_failure)?))
}

fn read_optional_digest(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<Sha256Digest>, WindowsError> {
    if reader.read_option_tag().map_err(codec_failure)? {
        read_digest(reader).map(Some)
    } else {
        Ok(None)
    }
}

fn write_windows_path(
    writer: &mut CanonicalWriter,
    path: &WindowsPath,
) -> Result<(), WindowsError> {
    #[cfg(target_os = "windows")]
    use std::os::windows::ffi::OsStrExt as _;

    #[cfg(target_os = "windows")]
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    #[cfg(not(target_os = "windows"))]
    let units = path.as_os_str().to_string_lossy().encode_utf16().collect::<Vec<_>>();
    let byte_count = units
        .len()
        .checked_mul(2)
        .ok_or_else(|| recovery_error("private secret recovery path length overflowed"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(byte_count)
        .map_err(|_| recovery_error("private secret recovery path allocation is unavailable"))?;
    for unit in units {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    writer.write_bytes(&bytes).map_err(codec_failure)
}

fn read_windows_path(reader: &mut CanonicalReader<'_>) -> Result<WindowsPath, WindowsError> {
    let bytes = reader.read_bytes().map_err(codec_failure)?;
    let chunks = bytes.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(recovery_error("private secret recovery path has an odd byte length"));
    }
    let mut units = Vec::new();
    units
        .try_reserve_exact(bytes.len() / 2)
        .map_err(|_| recovery_error("private secret recovery path allocation is unavailable"))?;
    units.extend(chunks.map(|pair| u16::from_be_bytes([pair[0], pair[1]])));
    #[cfg(target_os = "windows")]
    let native = {
        use std::os::windows::ffi::OsStringExt as _;
        std::ffi::OsString::from_wide(&units)
    };
    #[cfg(not(target_os = "windows"))]
    let native: std::ffi::OsString = String::from_utf16(&units)
        .map_err(|_| recovery_error("private secret recovery path is not representable"))?
        .into();
    WindowsPath::from_os_str(&native)
        .map_err(|_| recovery_error("private secret recovery path is invalid"))
}

fn codec_failure(_error: peritus_codec::CodecError) -> WindowsError {
    recovery_error("native recovery canonical codec rejected the record")
}

fn recovery_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Recover,
        WindowsRecovery::Quarantine,
        detail,
    )
}
