//! Atomic durable approve-once consumption persistence.

mod resolution;

use peritus_approval::{
    ApprovalAggregate, ApprovalChoice, ApprovalPhase, ApprovalUseOutcome, ApprovedActionTransition,
    ConsumedApproval,
};
use peritus_types::Sha256Digest;

use crate::{
    AppendRequest, CommittedBatch, CurrentCredentialRegistry, JournalError, JournalErrorKind,
    SqliteJournal, StateInstall,
    domain::{commit, encoding},
};

pub use resolution::{ApprovalUseResolution, ApprovalUseResolutionRequest};

const NAMESPACE: u16 = 104;
const DOMAIN: &[u8] = b"peritus.approval-use.v1";
const VALUE_KIND: u16 = 5;
const APPROVED_ONCE_PHASE_TAG: u8 = 2;
const CONSUMED_PHASE_TAG: u8 = 4;

/// Move-only request that adds one approve-once consumption to an existing atomic append.
pub struct ApprovalUseCommitRequest {
    append: AppendRequest,
    aggregate: ApprovalAggregate,
    transition: ApprovedActionTransition,
    consumed: ConsumedApproval,
    install: StateInstall,
    registry_binding: (u64, u64, Sha256Digest),
}

impl ApprovalUseCommitRequest {
    /// Binds a consumed approval successor to its durable predecessor and current registry.
    ///
    /// The supplied append may already contain multiple aggregate events and state installs.
    ///
    /// `C0` adds the approval state, and all records commit in the same `SQLite` transaction.
    ///
    /// # Errors
    ///
    /// Rejects a missing durable predecessor, inconsistent logical outcome, stale registry
    /// observation, or approval-state revision overflow.
    pub fn new(
        append: AppendRequest,
        outcome: ApprovalUseOutcome,
        expected_revision: u64,
        registry: &CurrentCredentialRegistry,
    ) -> Result<Self, JournalError> {
        if expected_revision == 0 {
            return Err(input("approval use requires a positive durable predecessor revision"));
        }
        let (aggregate, transition, consumed) = outcome.into_parts();
        validate_outcome(&aggregate, &transition, &consumed)?;
        let registry_binding = validate_registry(&aggregate, &transition, registry)?;
        let append = append.bind_registry_current(
            registry_binding.0,
            registry_binding.1,
            registry_binding.2,
        );
        let revision = commit::successor(Some(expected_revision))?;
        let key = aggregate.request().request_id().as_bytes().to_vec();
        let value = encode_use(&aggregate, &transition, &consumed, registry_binding)?;
        let install = StateInstall::new(NAMESPACE, key, Some(expected_revision), revision, value)?;
        Ok(Self { append, aggregate, transition, consumed, install, registry_binding })
    }
}

/// Opaque committed receipt for one atomically consumed approve-once action.
pub struct CommittedApprovalUse {
    batch: CommittedBatch,
    aggregate: ApprovalAggregate,
    transition: ApprovedActionTransition,
    consumed: ConsumedApproval,
    state_revision: u64,
    state_digest: Sha256Digest,
    registry_binding: (u64, u64, Sha256Digest),
}

impl CommittedApprovalUse {
    /// Borrows the complete committed multi-aggregate batch.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }

    /// Borrows the exact consumed successor aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &ApprovalAggregate {
        &self.aggregate
    }

    /// Borrows the logical approved-action transition.
    #[must_use]
    pub const fn transition(&self) -> &ApprovedActionTransition {
        &self.transition
    }

    /// Borrows the exact consumption record.
    #[must_use]
    pub const fn consumed(&self) -> &ConsumedApproval {
        &self.consumed
    }

    /// Returns the installed approval-state revision.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the digest of the durable consumed successor projection.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }

    /// Returns the bound `(registry revision, generation, snapshot digest)`.
    #[must_use]
    pub const fn registry_binding(&self) -> (u64, u64, Sha256Digest) {
        self.registry_binding
    }

    /// Consumes the receipt into its batch and exact move-only logical values.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (CommittedBatch, ApprovalAggregate, ApprovedActionTransition, ConsumedApproval) {
        (self.batch, self.aggregate, self.transition, self.consumed)
    }
}

impl SqliteJournal {
    /// Commits an existing append and approve-once consumption in one transaction.
    ///
    /// # Errors
    ///
    /// Returns stale-registry, state-CAS, idempotency, storage, or integrity failures.
    pub fn commit_approval_use(
        &mut self,
        request: ApprovalUseCommitRequest,
    ) -> Result<CommittedApprovalUse, JournalError> {
        let ApprovalUseCommitRequest {
            append,
            aggregate,
            transition,
            consumed,
            install,
            registry_binding,
        } = request;
        let current = self.current_credential_registry()?;
        if (current.revision(), current.generation(), current.digest()) != registry_binding {
            return Err(JournalError::new(
                JournalErrorKind::StaleRegistry,
                "commit approval use",
                "credential registry changed after approval-use planning",
            ));
        }
        let (batch, state) = commit::commit_state(self, append, DOMAIN, install)?;
        Ok(CommittedApprovalUse {
            batch,
            aggregate,
            transition,
            consumed,
            state_revision: state.revision(),
            state_digest: state.digest(),
            registry_binding,
        })
    }
}

fn approval_install(
    outcome: &ApprovalUseOutcome,
    expected_revision: u64,
    registry_binding: (u64, u64, Sha256Digest),
) -> Result<StateInstall, JournalError> {
    let revision = commit::successor(Some(expected_revision))?;
    let key = outcome.aggregate().request().request_id().as_bytes().to_vec();
    let value = encode_use(
        outcome.aggregate(),
        outcome.transition(),
        outcome.consumed(),
        registry_binding,
    )?;
    StateInstall::new(NAMESPACE, key, Some(expected_revision), revision, value)
}

fn validate_outcome(
    aggregate: &ApprovalAggregate,
    transition: &ApprovedActionTransition,
    consumed: &ConsumedApproval,
) -> Result<(), JournalError> {
    let request = aggregate.request();
    let resolution = aggregate
        .resolution()
        .ok_or_else(|| input("consumed approval has no authenticated resolution"))?;
    if aggregate.phase() != ApprovalPhase::Consumed
        || resolution.choice() != ApprovalChoice::ApproveOnce
        || transition.request_id() != request.request_id()
        || transition.request_digest() != request.digest()
        || transition.action_id() != request.action_id()
        || transition.action_digest() != request.action_digest()
        || transition.revision() != request.scope().revision()
        || transition.decision_digest() != resolution.decision_digest()
        || transition.command_id() != resolution.command_id()
        || transition.registry_revision() != resolution.registry_revision()
        || transition.valid_until() != resolution.valid_until()
        || consumed.request_id() != request.request_id()
        || consumed.decision_digest() != resolution.decision_digest()
        || consumed.action_id() != request.action_id()
    {
        return Err(input("approval-use outcome contains inconsistent authority bindings"));
    }
    Ok(())
}

fn validate_registry(
    aggregate: &ApprovalAggregate,
    transition: &ApprovedActionTransition,
    current: &CurrentCredentialRegistry,
) -> Result<(u64, u64, Sha256Digest), JournalError> {
    let resolution = aggregate
        .resolution()
        .ok_or_else(|| input("consumed approval has no authenticated resolution"))?;
    if transition.registry_revision().get() != current.revision()
        || resolution.registry_revision().get() != current.revision()
        || resolution.registry_digest() != current.digest()
    {
        return Err(JournalError::new(
            JournalErrorKind::StaleRegistry,
            "plan approval use",
            "approval use is not bound to the exact current credential registry",
        ));
    }
    Ok((current.revision(), current.generation(), current.digest()))
}

fn encode_use(
    aggregate: &ApprovalAggregate,
    transition: &ApprovedActionTransition,
    consumed: &ConsumedApproval,
    registry: (u64, u64, Sha256Digest),
) -> Result<Vec<u8>, JournalError> {
    let request = aggregate.request();
    let resolution = aggregate
        .resolution()
        .ok_or_else(|| input("consumed approval has no authenticated resolution"))?;
    let mut payload = Vec::with_capacity(512);
    payload.extend_from_slice(request.request_id().as_bytes());
    payload.extend_from_slice(request.action_id().as_bytes());
    encoding::digest(&mut payload, request.action_digest().sha256());
    encoding::digest(&mut payload, request.digest().sha256());
    encoding::revision(&mut payload, request.scope().revision());
    encoding::u8_value(&mut payload, APPROVED_ONCE_PHASE_TAG);
    encoding::u8_value(&mut payload, CONSUMED_PHASE_TAG);
    encoding::digest(&mut payload, resolution.decision_digest().sha256());
    payload.extend_from_slice(resolution.command_id().as_bytes());
    encoding::u64_value(&mut payload, resolution.registry_revision().get());
    encoding::digest(&mut payload, resolution.registry_digest());
    encoding::u64_value(&mut payload, resolution.credential_generation().get());
    encoding::instant(&mut payload, resolution.valid_until());
    encoding::u64_value(&mut payload, request.authority_time().epoch().get());
    encoding::u64_value(&mut payload, request.authority_time().greatest_tick_millis());
    payload.extend_from_slice(transition.request_id().as_bytes());
    encoding::digest(&mut payload, transition.request_digest().sha256());
    payload.extend_from_slice(transition.action_id().as_bytes());
    encoding::digest(&mut payload, transition.action_digest().sha256());
    encoding::revision(&mut payload, transition.revision());
    encoding::digest(&mut payload, transition.decision_digest().sha256());
    payload.extend_from_slice(transition.command_id().as_bytes());
    encoding::u64_value(&mut payload, transition.registry_revision().get());
    encoding::instant(&mut payload, transition.valid_until());
    payload.extend_from_slice(consumed.request_id().as_bytes());
    encoding::digest(&mut payload, consumed.decision_digest().sha256());
    payload.extend_from_slice(consumed.action_id().as_bytes());
    encoding::u64_value(&mut payload, registry.0);
    encoding::u64_value(&mut payload, registry.1);
    encoding::digest(&mut payload, registry.2);
    Ok(encoding::value(VALUE_KIND, &payload))
}

const fn input(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "plan approval use", detail)
}
