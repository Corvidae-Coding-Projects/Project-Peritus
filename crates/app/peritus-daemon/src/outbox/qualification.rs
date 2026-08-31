//! Bounded public-admin qualification for an effect-before-ack outbox restart.

mod blob;
mod effect;
mod gate;
mod journal;
mod journal_before;
mod lease;
mod patch;
mod snapshot;

use std::path::{Path, PathBuf};

use peritus_codec::{CodecLimits, encode_frame, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendPlan, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, OutboxDraft, OutboxId, OutboxMessage, OutboxState, SqliteJournal, StoreId,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

pub use self::blob::{
    recover_blob_after_crash, recover_blob_before_crash, stage_blob_after_crash,
    stage_blob_before_crash,
};
use self::effect::QualificationDestination;
pub use self::gate::{
    recover_gate_after_crash, recover_gate_before_crash, stage_gate_after_crash,
    stage_gate_before_crash,
};
use self::journal::{
    acquire_instance, journal_error, open_journal, verify_empty_journal,
    verify_empty_journal_for_store,
};

pub use self::journal_before::{recover_journal_before_crash, stage_journal_before_crash};
pub use self::lease::{
    recover_lease_after_crash, recover_lease_before_crash, stage_lease_after_crash,
    stage_lease_before_crash,
};
pub use self::patch::{
    recover_patch_after_crash, recover_patch_before_crash, stage_patch_after_crash,
    stage_patch_before_crash,
};
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};
pub use snapshot::{
    recover_snapshot_after_crash, recover_snapshot_before_crash, stage_snapshot_after_crash,
    stage_snapshot_before_crash,
};

const DESTINATION: &str = "peritus.admin.outbox-crash.v1";
const FRAME_FAMILY: u16 = 65_001;
const MAX_ATTEMPTS: u16 = 2;
const FIRST_CLAIM_NOW: u64 = 1;
const FIRST_CLAIM_LEASE: u64 = 2;
const RECOVERY_CLAIM_NOW: u64 = FIRST_CLAIM_LEASE;
const RECOVERY_CLAIM_LEASE: u64 = 3;
const OUTBOX_ID_BYTES: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];

/// Durable checkpoint returned only after the external effect and before acknowledgement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxCrashCheckpoint {
    effect_path: PathBuf,
    claim_fence: u64,
}

impl OutboxCrashCheckpoint {
    /// Returns the exact identity-bearing external effect path.
    pub(crate) fn effect_path(&self) -> &Path {
        &self.effect_path
    }

    /// Returns the fence deliberately left unsettled by the first process.
    pub(crate) const fn claim_fence(&self) -> u64 {
        self.claim_fence
    }
}

/// Direct facts observed after effect-before-ack recovery and exact settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutboxCrashQualification {
    destination_reconciled: bool,
    external_effects: u64,
    duplicate_effects: u64,
    exact_fence_acknowledged: bool,
    pending_claims: u64,
}

impl OutboxCrashQualification {
    /// Returns whether recovery checked the exact destination identity before settlement.
    pub(crate) const fn destination_reconciled(self) -> bool {
        self.destination_reconciled
    }

    /// Returns the number of exact identity-bearing filesystem effects.
    pub(crate) const fn external_effects(self) -> u64 {
        self.external_effects
    }

    /// Returns additional files observed in the dedicated effect destination.
    pub(crate) const fn duplicate_effects(self) -> u64 {
        self.duplicate_effects
    }

    /// Returns whether C0 accepted acknowledgement of the exact live recovery fence.
    pub(crate) const fn exact_fence_acknowledged(self) -> bool {
        self.exact_fence_acknowledged
    }

    /// Returns unsettled qualification claims after the exact acknowledgement.
    pub(crate) const fn pending_claims(self) -> u64 {
        self.pending_claims
    }
}

/// Seeds and claims one bounded C0 delivery, performs its idempotent filesystem effect, and
/// deliberately returns without acknowledging the live fence.
///
/// The caller must terminate this admin invocation after observing the returned checkpoint. A
/// later process calls [`recover_outbox_crash`] against the same configuration.
pub fn stage_outbox_crash(config: &DaemonConfig) -> Result<OutboxCrashCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = QualificationIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let destination = QualificationDestination::prepare(
        config.paths().state_root(),
        identity.outbox_id,
        &identity.payload,
    )?;
    let mut journal = open_journal(config, store_id)?;
    seed(&mut journal, &identity)?;
    let message = journal
        .claim_outbox(FIRST_CLAIM_NOW, FIRST_CLAIM_LEASE)
        .map_err(journal_error)?
        .ok_or_else(|| qualification_error("seeded qualification delivery was not claimable"))?;
    let fence = validate_claim(&message, &identity, 1)?;
    destination.apply_once(&identity.payload)?;
    let observed = destination.reconcile(&identity.payload)?;
    if observed.external_effects() != 1 || observed.duplicate_effects() != 0 {
        return Err(qualification_error(
            "qualification destination does not contain exactly one external effect",
        ));
    }
    Ok(OutboxCrashCheckpoint {
        effect_path: destination.effect_path().to_path_buf(),
        claim_fence: fence,
    })
}

/// Reclaims the delivery after its first lease, reconciles the exact filesystem identity before
/// retry, and acknowledges C0's new live fence.
pub fn recover_outbox_crash(
    config: &DaemonConfig,
) -> Result<OutboxCrashQualification, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = QualificationIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let destination = QualificationDestination::prepare(
        config.paths().state_root(),
        identity.outbox_id,
        &identity.payload,
    )?;
    let mut journal = open_journal(config, store_id)?;
    let message = journal
        .claim_outbox(RECOVERY_CLAIM_NOW, RECOVERY_CLAIM_LEASE)
        .map_err(journal_error)?
        .ok_or_else(|| {
            qualification_error("unsettled qualification delivery was not reclaimable")
        })?;
    let fence = validate_claim(&message, &identity, 2)?;

    // Reconciliation is intentionally complete before any effect retry or acknowledgement.
    let observed = destination.reconcile(&identity.payload)?;
    if observed.external_effects() != 1 || observed.duplicate_effects() != 0 {
        return Err(qualification_error(
            "recovery did not observe exactly one identity-bound external effect",
        ));
    }
    journal.acknowledge_outbox(identity.outbox_id, fence).map_err(journal_error)?;
    let pending_claims = u64::from(
        journal
            .claim_outbox(RECOVERY_CLAIM_LEASE, RECOVERY_CLAIM_LEASE + 1)
            .map_err(journal_error)?
            .is_some(),
    );

    Ok(OutboxCrashQualification {
        destination_reconciled: true,
        external_effects: observed.external_effects(),
        duplicate_effects: observed.duplicate_effects(),
        exact_fence_acknowledged: true,
        pending_claims,
    })
}

struct QualificationIdentity {
    aggregate: AggregateKey,
    command_id: CommandId,
    event_id: EventId,
    outbox_id: OutboxId,
    request_digest: Sha256Digest,
    revision_digest: Sha256Digest,
    payload: Vec<u8>,
}

impl QualificationIdentity {
    fn new(store_id: StoreId) -> Result<Self, DaemonError> {
        let outbox_id = OutboxId::new(OUTBOX_ID_BYTES).map_err(journal_error)?;
        let payload = effect_payload(store_id, outbox_id);
        let aggregate_id =
            AggregateId::new(identifier(b"peritus/g0/outbox-crash-aggregate/v1\0", store_id))
                .map_err(journal_error)?;
        let command_id =
            CommandId::new(identifier(b"peritus/g0/outbox-crash-command/v1\0", store_id))
                .map_err(|_| identity_error("derive qualification command identity"))?;
        let event_id = EventId::new(identifier(b"peritus/g0/outbox-crash-event/v1\0", store_id))
            .map_err(|_| identity_error("derive qualification event identity"))?;
        let request_digest = request_digest(store_id, outbox_id, &payload);
        let revision_digest = digest(b"peritus/g0/outbox-crash-revision/v1\0", store_id);
        Ok(Self {
            aggregate: AggregateKey::new(AggregateKind::Application, aggregate_id),
            command_id,
            event_id,
            outbox_id,
            request_digest,
            revision_digest,
            payload,
        })
    }
}

fn seed(journal: &mut SqliteJournal, identity: &QualificationIdentity) -> Result<(), DaemonError> {
    let plan = build_plan(journal, identity)?;
    journal.append(plan).map(|_| ()).map_err(journal_error)
}

fn build_plan(
    journal: &SqliteJournal,
    identity: &QualificationIdentity,
) -> Result<AppendPlan, DaemonError> {
    let frame = ExactFrame::new(
        encode_frame(FRAME_FAMILY, 1, &identity.payload, CodecLimits::PRODUCTION)
            .map_err(|error| codec_error("encode qualification event", error))?,
    )
    .map_err(journal_error)?;
    let event = EventDraft::new(
        identity.aggregate,
        EventSequence::first(),
        identity.event_id,
        None,
        frame,
        identity.revision_digest,
        Vec::new(),
    )
    .map_err(journal_error)?;
    let outbox = OutboxDraft::new(
        identity.outbox_id,
        DESTINATION.to_owned(),
        identity.payload.clone(),
        MAX_ATTEMPTS,
    )
    .map_err(journal_error)?;
    AppendRequest::new(
        journal.store_id(),
        identity.command_id,
        identity.request_digest,
        vec![HeadExpectation::Absent(identity.aggregate)],
        vec![event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        vec![outbox],
    )
    .plan()
    .map_err(journal_error)
}

fn validate_claim(
    message: &OutboxMessage,
    identity: &QualificationIdentity,
    expected_attempt: u16,
) -> Result<u64, DaemonError> {
    if message.id() != identity.outbox_id
        || message.destination() != DESTINATION
        || message.payload() != identity.payload.as_slice()
        || message.max_attempts() != MAX_ATTEMPTS
        || message.attempts() != expected_attempt
        || message.state() != OutboxState::Claimed
    {
        return Err(qualification_error(
            "C0 returned a claim other than the exact qualification delivery",
        ));
    }
    message
        .fence()
        .filter(|fence| *fence > 0)
        .ok_or_else(|| qualification_error("qualification claim has no positive live fence"))
}

fn effect_payload(store_id: StoreId, outbox_id: OutboxId) -> Vec<u8> {
    let mut payload = Vec::with_capacity(75);
    payload.extend_from_slice(b"peritus/g0/outbox-crash-effect/v1\0");
    payload.extend_from_slice(store_id.as_bytes());
    payload.extend_from_slice(outbox_id.as_bytes());
    payload
}

fn request_digest(store_id: StoreId, outbox_id: OutboxId, payload: &[u8]) -> Sha256Digest {
    let mut binding = Vec::with_capacity(DESTINATION.len() + payload.len() + 64);
    binding.extend_from_slice(b"peritus/g0/outbox-crash-request/v1\0");
    binding.extend_from_slice(store_id.as_bytes());
    binding.extend_from_slice(outbox_id.as_bytes());
    binding.extend_from_slice(DESTINATION.as_bytes());
    binding.extend_from_slice(&MAX_ATTEMPTS.to_be_bytes());
    binding.extend_from_slice(payload);
    sha256(&binding)
}

fn identifier(domain: &[u8], store_id: StoreId) -> [u8; 16] {
    let digest = digest(domain, store_id);
    let mut identifier = [0_u8; 16];
    identifier.copy_from_slice(&digest.as_bytes()[..16]);
    identifier
}

fn digest(domain: &[u8], store_id: StoreId) -> Sha256Digest {
    let mut binding = Vec::with_capacity(domain.len() + 16);
    binding.extend_from_slice(domain);
    binding.extend_from_slice(store_id.as_bytes());
    sha256(&binding)
}

fn digest_hex(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn qualification_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify effect-before-ack outbox recovery",
        detail,
    )
}

fn identity_error(operation: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        operation,
        "domain-separated qualification identity is invalid",
    )
}

fn codec_error(operation: &'static str, error: peritus_codec::CodecError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        operation,
        "bounded qualification frame could not be encoded",
        error,
    )
}
