//! Versioned native recovery record and exact ownership classification.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits, decode_frame, encode_frame};
use peritus_process::ProcessTreeIdentity;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsPhase, WindowsRecovery};

const FAMILY: u16 = 0xC317;
const SCHEMA_V1: u16 = 1;
const SCHEMA_V2: u16 = 2;
const CHECKSUM_BYTES: usize = Sha256Digest::LENGTH;
const LIMITS: CodecLimits = CodecLimits::new(4_096, 4_080, 32, 512, 512, 4);

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

/// Durable C3 supporting recovery evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsRecoveryRecord {
    schema: u16,
    identity: RuntimeIdentity,
    phase: WindowsPhase,
    acl_restored: bool,
    secret_files_removed: bool,
    helper_reaped: bool,
    tree: Option<ProcessTreeIdentity>,
    acl_transaction_digest: Option<Sha256Digest>,
    acl_receipt: Option<Sha256Digest>,
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
    canonical: Vec<u8>,
}

impl WindowsRecoveryRecord {
    /// Creates a prepared recovery record.
    #[must_use]
    pub fn prepared(identity: RuntimeIdentity) -> Self {
        let mut value = Self {
            schema: SCHEMA_V2,
            identity,
            phase: WindowsPhase::Prepared,
            acl_restored: false,
            secret_files_removed: false,
            helper_reaped: false,
            tree: None,
            acl_transaction_digest: None,
            acl_receipt: None,
            owner_operation_digest: None,
            service_owner_digest: None,
            canonical: Vec::new(),
        };
        value.canonical = value.encode().unwrap_or_default();
        value
    }

    pub(crate) fn prepared_owned(
        identity: RuntimeIdentity,
        transaction_digest: Option<Sha256Digest>,
        receipt: Option<Sha256Digest>,
        owner_operation_digest: Option<Sha256Digest>,
        service_owner_digest: Option<Sha256Digest>,
    ) -> Self {
        let complete_transaction = transaction_digest.is_some() == receipt.is_some();
        let complete_owner = owner_operation_digest.is_some() == service_owner_digest.is_some();
        let mut value = Self {
            schema: SCHEMA_V2,
            identity,
            phase: WindowsPhase::Prepared,
            acl_restored: false,
            secret_files_removed: false,
            helper_reaped: false,
            tree: None,
            acl_transaction_digest: complete_transaction.then_some(transaction_digest).flatten(),
            acl_receipt: complete_transaction.then_some(receipt).flatten(),
            owner_operation_digest: complete_owner.then_some(owner_operation_digest).flatten(),
            service_owner_digest: complete_owner.then_some(service_owner_digest).flatten(),
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
        if !crate::verified::recovery_advance_allowed(self.phase.ordinal(), phase.ordinal())
            || (phase == WindowsPhase::Released
                && !(acl_restored && secret_files_removed && helper_reaped))
        {
            return Err(recovery_error("native recovery lifecycle or cleanup facts are invalid"));
        }
        self.phase = phase;
        self.acl_restored = acl_restored;
        self.secret_files_removed = secret_files_removed;
        self.helper_reaped = helper_reaped;
        self.canonical = self.encode()?;
        Ok(())
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
        self.acl_restored = true;
        self.secret_files_removed = true;
        self.helper_reaped = true;
        self.canonical = self.encode()?;
        Ok(())
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
        if frame.header().family() != FAMILY || !matches!(schema, SCHEMA_V1 | SCHEMA_V2) {
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
        let (
            tree,
            acl_transaction_digest,
            acl_receipt,
            owner_operation_digest,
            service_owner_digest,
        ) = if schema == SCHEMA_V2 {
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
        reader.finish().map_err(codec_failure)?;
        let any_cleanup = acl_restored || secret_files_removed || helper_reaped;
        let complete_cleanup = acl_restored && secret_files_removed && helper_reaped;
        if (phase == WindowsPhase::Released || any_cleanup) && !complete_cleanup {
            return Err(recovery_error("native recovery record has inconsistent cleanup facts"));
        }
        let value = Self {
            schema,
            identity,
            phase,
            acl_restored,
            secret_files_removed,
            helper_reaped,
            tree,
            acl_transaction_digest,
            acl_receipt,
            owner_operation_digest,
            service_owner_digest,
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
    /// Reports complete teardown evidence.
    #[must_use]
    pub const fn cleanup_complete(&self) -> bool {
        self.acl_restored && self.secret_files_removed && self.helper_reaped
    }

    pub(crate) const fn custody_complete(&self) -> bool {
        self.owner_operation_digest.is_some()
            && self.service_owner_digest.is_some()
            && self.acl_transaction_digest.is_some()
            && self.acl_receipt.is_some()
            && self.phase != WindowsPhase::Released
            && !self.cleanup_complete()
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
        writer.write_bool(self.acl_restored).map_err(codec_failure)?;
        writer.write_bool(self.secret_files_removed).map_err(codec_failure)?;
        writer.write_bool(self.helper_reaped).map_err(codec_failure)?;
        if self.schema == SCHEMA_V2 {
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
        (Some(record), RecoveryProbe::LiveOwned(identity)) if record.identity == identity => {
            RecoveryClassification::LiveOwned
        }
        (Some(record), RecoveryProbe::Absent) if record.cleanup_complete() => {
            RecoveryClassification::AbsentClean
        }
        (Some(_), RecoveryProbe::LiveOwned(_) | RecoveryProbe::Mismatched) => {
            RecoveryClassification::Mismatched
        }
        _ => RecoveryClassification::Indeterminate,
    }
}

fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, WindowsError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec_failure)?))
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
