//! Commit-once lease compare-and-swap persistence.

use peritus_leases::{
    FenceCause, LeaseAggregate, LeasePhase, LeaseScope, LeaseTransition, LeaseTransitionKind,
    ReconciliationCorrelation, ReconciliationDisposition, RetirementReason,
};
use peritus_types::Sha256Digest;

use crate::{
    AppendRequest, CommittedBatch, JournalError, JournalErrorKind, SqliteJournal, StateInstall,
    domain::{commit, encoding},
};

const NAMESPACE: u16 = 103;
const DOMAIN: &[u8] = b"peritus.lease-transition.v1";
const VALUE_KIND: u16 = 3;

/// Move-only request binding the B1 lease record's exact version CAS to the journal transaction.
pub struct LeaseCommitRequest {
    append: AppendRequest,
    transition: LeaseTransition,
    install: StateInstall,
}

impl LeaseCommitRequest {
    /// Creates a request directly from the verified lease record's before/after versions.
    ///
    /// # Errors
    ///
    /// Returns an input error if the transition's successor disagrees with its record or its
    /// version is not the exact compare-and-swap successor.
    pub fn new(append: AppendRequest, transition: LeaseTransition) -> Result<Self, JournalError> {
        let record = transition.record();
        let expected_revision = record.before_version().map(peritus_types::RevisionNumber::get);
        let revision = record.after_version().get();
        if transition.next().scope() != record.scope()
            || transition.next().version() != record.after_version()
            || transition.next().generation() != record.after_generation()
            || transition.next().phase() != record.after_phase()
            || commit::successor(expected_revision)? != revision
        {
            return Err(input("lease record and successor aggregate disagree"));
        }
        let key = scope_key(record.scope());
        let value = encode_transition(&transition);
        let install = StateInstall::new(NAMESPACE, key, expected_revision, revision, value)?;
        Ok(Self { append, transition, install })
    }
}

/// Opaque committed lease transition retaining the exact move-only successor.
pub struct CommittedLeaseTransition {
    batch: CommittedBatch,
    transition: LeaseTransition,
    state_revision: u64,
    state_digest: Sha256Digest,
}

impl CommittedLeaseTransition {
    /// Borrows the exact committed event batch.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }

    /// Borrows the exact logical transition whose successor became durable.
    #[must_use]
    pub const fn transition(&self) -> &LeaseTransition {
        &self.transition
    }

    /// Returns the installed lease CAS revision.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the digest of the complete durable lease projection.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }

    /// Consumes the receipt into its committed batch and exact next lease aggregate.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, LeaseAggregate) {
        (self.batch, self.transition.into_next())
    }
}

impl SqliteJournal {
    /// Commits one verified lease transition under its exact version CAS.
    ///
    /// # Errors
    ///
    /// Returns journal CAS, idempotency, storage, or integrity failures.
    pub fn commit_lease_transition(
        &mut self,
        request: LeaseCommitRequest,
    ) -> Result<CommittedLeaseTransition, JournalError> {
        let LeaseCommitRequest { append, transition, install } = request;
        let (batch, state) = commit::commit_state(self, append, DOMAIN, install)?;
        Ok(CommittedLeaseTransition {
            batch,
            transition,
            state_revision: state.revision(),
            state_digest: state.digest(),
        })
    }
}

fn encode_transition(transition: &LeaseTransition) -> Vec<u8> {
    let record = transition.record();
    let next = transition.next();
    let mut payload = Vec::with_capacity(384);
    payload.extend_from_slice(record.command_id().as_bytes());
    scope(&mut payload, record.scope());
    encoding::optional_u64(
        &mut payload,
        record.before_version().map(peritus_types::RevisionNumber::get),
    );
    encoding::u64_value(&mut payload, record.after_version().get());
    encoding::optional_u64(
        &mut payload,
        record.before_generation().map(peritus_types::Generation::get),
    );
    encoding::u64_value(&mut payload, record.after_generation().get());
    optional_phase(&mut payload, record.before_phase());
    encoding::u8_value(&mut payload, phase_tag(record.after_phase()));
    transition_kind(&mut payload, record.kind());
    encoding::u64_value(&mut payload, next.generation().get());
    encoding::u64_value(&mut payload, next.version().get());
    encoding::u64_value(&mut payload, next.authority_time().epoch().get());
    encoding::u64_value(&mut payload, next.authority_time().greatest_tick_millis());
    encoding::u8_value(&mut payload, phase_tag(next.phase()));
    if let Some(active) = next.active() {
        encoding::u8_value(&mut payload, 1);
        claim(&mut payload, active.claim());
    } else {
        encoding::u8_value(&mut payload, 0);
    }
    if let Some(reconciliation) = next.reconciliation() {
        encoding::u8_value(&mut payload, 1);
        correlation(&mut payload, reconciliation.correlation());
        encoding::u8_value(&mut payload, cause_tag(reconciliation.cause()));
    } else {
        encoding::u8_value(&mut payload, 0);
    }
    if let Some(quarantine) = next.quarantine() {
        encoding::u8_value(&mut payload, 1);
        correlation(&mut payload, quarantine.correlation());
        encoding::u8_value(&mut payload, cause_tag(quarantine.cause()));
        disposition(&mut payload, quarantine.disposition());
    } else {
        encoding::u8_value(&mut payload, 0);
    }
    match next.retirement_reason() {
        Some(reason) => {
            encoding::u8_value(&mut payload, 1);
            encoding::u8_value(&mut payload, retirement_tag(reason));
        }
        None => encoding::u8_value(&mut payload, 0),
    }
    encoding::value(VALUE_KIND, &payload)
}

fn scope_key(value: LeaseScope) -> Vec<u8> {
    let mut key = Vec::with_capacity(48);
    key.extend_from_slice(value.workspace_id().as_bytes());
    key.extend_from_slice(value.resource_id().as_bytes());
    key.extend_from_slice(value.environment_id().as_bytes());
    key
}

fn scope(bytes: &mut Vec<u8>, value: LeaseScope) {
    bytes.extend_from_slice(&scope_key(value));
}

fn claim(bytes: &mut Vec<u8>, value: peritus_leases::LeaseClaim) {
    scope(bytes, value.scope());
    bytes.extend_from_slice(value.holder().actor_id().as_bytes());
    bytes.extend_from_slice(value.holder().session_id().as_bytes());
    encoding::u64_value(bytes, value.generation().get());
    encoding::u64_value(bytes, value.claim_version().get());
    encoding::instant(bytes, value.issued_at());
    encoding::instant(bytes, value.expires_at());
}

fn correlation(bytes: &mut Vec<u8>, value: ReconciliationCorrelation) {
    scope(bytes, value.scope());
    encoding::u64_value(bytes, value.fenced_generation().get());
    bytes.extend_from_slice(value.prior_holder().actor_id().as_bytes());
    bytes.extend_from_slice(value.prior_holder().session_id().as_bytes());
}

fn disposition(bytes: &mut Vec<u8>, value: ReconciliationDisposition) {
    match value {
        ReconciliationDisposition::SafeToAcquire { holder_quiescence, resource_safety } => {
            encoding::u8_value(bytes, 1);
            bytes.extend_from_slice(holder_quiescence.as_bytes());
            bytes.extend_from_slice(resource_safety.as_bytes());
        }
        ReconciliationDisposition::Dirty { evidence_id } => {
            encoding::u8_value(bytes, 2);
            bytes.extend_from_slice(evidence_id.as_bytes());
        }
        ReconciliationDisposition::Indeterminate { evidence_id } => {
            encoding::u8_value(bytes, 3);
            bytes.extend_from_slice(evidence_id.as_bytes());
        }
    }
}

fn transition_kind(bytes: &mut Vec<u8>, kind: LeaseTransitionKind) {
    match kind {
        LeaseTransitionKind::Minted => encoding::u8_value(bytes, 1),
        LeaseTransitionKind::Acquired => encoding::u8_value(bytes, 2),
        LeaseTransitionKind::Renewed => encoding::u8_value(bytes, 3),
        LeaseTransitionKind::Used { action_id, action_digest } => {
            encoding::u8_value(bytes, 4);
            bytes.extend_from_slice(action_id.as_bytes());
            encoding::digest(bytes, action_digest);
        }
        LeaseTransitionKind::ReleasedAvailable => encoding::u8_value(bytes, 5),
        LeaseTransitionKind::ReleasedReconciling => encoding::u8_value(bytes, 6),
        LeaseTransitionKind::Expired => encoding::u8_value(bytes, 7),
        LeaseTransitionKind::HolderLost => encoding::u8_value(bytes, 8),
        LeaseTransitionKind::ClockDiscontinuity => encoding::u8_value(bytes, 9),
        LeaseTransitionKind::Revoked => encoding::u8_value(bytes, 10),
        LeaseTransitionKind::ReconciledAvailable => encoding::u8_value(bytes, 11),
        LeaseTransitionKind::ReconciledQuarantined => encoding::u8_value(bytes, 12),
        LeaseTransitionKind::Retired(reason) => {
            encoding::u8_value(bytes, 13);
            encoding::u8_value(bytes, retirement_tag(reason));
        }
    }
}

fn optional_phase(bytes: &mut Vec<u8>, phase: Option<LeasePhase>) {
    match phase {
        Some(phase) => {
            encoding::u8_value(bytes, 1);
            encoding::u8_value(bytes, phase_tag(phase));
        }
        None => encoding::u8_value(bytes, 0),
    }
}

const fn phase_tag(phase: LeasePhase) -> u8 {
    match phase {
        LeasePhase::Available => 1,
        LeasePhase::Active => 2,
        LeasePhase::Reconciling => 3,
        LeasePhase::Quarantined => 4,
        LeasePhase::Retired => 5,
    }
}

const fn cause_tag(cause: FenceCause) -> u8 {
    match cause {
        FenceCause::ReleasedWithoutQuiescence => 1,
        FenceCause::Expired => 2,
        FenceCause::HolderLost => 3,
        FenceCause::ClockDiscontinuity => 4,
        FenceCause::Revoked => 5,
    }
}

const fn retirement_tag(reason: RetirementReason) -> u8 {
    match reason {
        RetirementReason::GenerationExhausted => 1,
        RetirementReason::VersionExhausted => 2,
    }
}

const fn input(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "plan lease commit", detail)
}
