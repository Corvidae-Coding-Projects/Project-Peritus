//! Commit-once human-approval transition persistence.

use peritus_approval::{
    ApprovalAggregate, ApprovalPhase, ApprovalTransition, ApprovalTransitionKind,
    ApprovalTransitionOutcome,
};
use peritus_types::Sha256Digest;

use crate::{
    AppendRequest, CommittedBatch, CurrentCredentialRegistry, JournalError, JournalErrorKind,
    SqliteJournal, StateInstall,
    domain::{commit, encoding},
};

const NAMESPACE: u16 = 104;
const DOMAIN: &[u8] = b"peritus.approval-transition.v1";
const VALUE_KIND: u16 = 4;

/// Move-only request binding one verified approval outcome to current credential-registry state.
pub struct ApprovalCommitRequest {
    append: AppendRequest,
    aggregate: ApprovalAggregate,
    transition: ApprovalTransition,
    install: StateInstall,
    registry_binding: Option<(u64, u64, Sha256Digest)>,
}

impl ApprovalCommitRequest {
    /// Binds an approval successor to an exact state CAS and current registry observation.
    ///
    /// # Errors
    ///
    /// Returns an input error if authenticated resolution facts do not match the supplied opaque
    /// current-registry observation, or if the state revision cannot advance exactly once.
    pub fn new(
        append: AppendRequest,
        outcome: ApprovalTransitionOutcome,
        expected_revision: Option<u64>,
        registry: Option<&CurrentCredentialRegistry>,
    ) -> Result<Self, JournalError> {
        let (aggregate, transition) = outcome.into_parts();
        let registry_binding = validate_registry(&aggregate, &transition, registry)?;
        let append = match registry_binding {
            Some((revision, generation, digest)) => {
                append.bind_registry_current(revision, generation, digest)
            }
            None => append,
        };
        let revision = commit::successor(expected_revision)?;
        let key = aggregate.request().request_id().as_bytes().to_vec();
        let value = encode_transition(&aggregate, &transition, registry_binding);
        let install = StateInstall::new(NAMESPACE, key, expected_revision, revision, value)?;
        Ok(Self { append, aggregate, transition, install, registry_binding })
    }
}

/// Opaque committed approval transition retaining the exact move-only successor aggregate.
pub struct CommittedApprovalTransition {
    batch: CommittedBatch,
    aggregate: ApprovalAggregate,
    transition: ApprovalTransition,
    state_revision: u64,
    state_digest: Sha256Digest,
    registry_binding: Option<(u64, u64, Sha256Digest)>,
}

impl CommittedApprovalTransition {
    /// Borrows the exact committed event batch.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }

    /// Borrows the exact committed logical successor aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &ApprovalAggregate {
        &self.aggregate
    }

    /// Borrows the exact logical transition record.
    #[must_use]
    pub const fn transition(&self) -> &ApprovalTransition {
        &self.transition
    }

    /// Returns the installed approval state revision.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the digest of the canonical durable approval state.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }

    /// Returns the exact bound `(registry revision, credential generation, snapshot digest)`.
    #[must_use]
    pub const fn registry_binding(&self) -> Option<(u64, u64, Sha256Digest)> {
        self.registry_binding
    }

    /// Consumes the receipt into the committed batch and exact successor aggregate.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, ApprovalAggregate) {
        (self.batch, self.aggregate)
    }
}

impl SqliteJournal {
    /// Commits one approval transition under its exact state and registry-currentness bindings.
    ///
    /// # Errors
    ///
    /// Returns journal CAS, idempotency, storage, or integrity failures.
    pub fn commit_approval_transition(
        &mut self,
        request: ApprovalCommitRequest,
    ) -> Result<CommittedApprovalTransition, JournalError> {
        let ApprovalCommitRequest { append, aggregate, transition, install, registry_binding } =
            request;
        if let Some((revision, generation, digest)) = registry_binding {
            let current = self.current_credential_registry()?;
            if current.revision() != revision
                || current.generation() != generation
                || current.digest() != digest
            {
                return Err(JournalError::new(
                    JournalErrorKind::StaleRegistry,
                    "commit approval transition",
                    "credential registry changed after approval planning",
                ));
            }
        }
        let (batch, state) = commit::commit_state(self, append, DOMAIN, install)?;
        Ok(CommittedApprovalTransition {
            batch,
            aggregate,
            transition,
            state_revision: state.revision(),
            state_digest: state.digest(),
            registry_binding,
        })
    }
}

fn validate_registry(
    aggregate: &ApprovalAggregate,
    transition: &ApprovalTransition,
    current: Option<&CurrentCredentialRegistry>,
) -> Result<Option<(u64, u64, Sha256Digest)>, JournalError> {
    match (transition.registry_revision(), aggregate.resolution(), current) {
        (None, None, None | Some(_)) => Ok(None),
        (Some(revision), Some(resolution), Some(current))
            if revision.get() == current.revision()
                && resolution.registry_revision().get() == current.revision()
                && resolution.registry_digest() == current.digest() =>
        {
            Ok(Some((current.revision(), current.generation(), current.digest())))
        }
        _ => Err(JournalError::new(
            JournalErrorKind::StaleRegistry,
            "plan approval commit",
            "approval resolution is not bound to the exact current credential registry",
        )),
    }
}

fn encode_transition(
    aggregate: &ApprovalAggregate,
    transition: &ApprovalTransition,
    registry: Option<(u64, u64, Sha256Digest)>,
) -> Vec<u8> {
    let request = aggregate.request();
    let mut payload = Vec::with_capacity(256);
    payload.extend_from_slice(request.request_id().as_bytes());
    payload.extend_from_slice(request.action_id().as_bytes());
    encoding::digest(&mut payload, request.action_digest().sha256());
    encoding::digest(&mut payload, request.digest().sha256());
    encoding::u8_value(&mut payload, transition_kind_tag(transition.kind()));
    encoding::u8_value(&mut payload, phase_tag(transition.from()));
    encoding::u8_value(&mut payload, phase_tag(transition.to()));
    encoding::u8_value(&mut payload, phase_tag(aggregate.phase()));
    encoding::optional_digest(
        &mut payload,
        transition.decision_digest().map(peritus_approval::ApprovalDecisionDigest::sha256),
    );
    encoding::optional_u64(
        &mut payload,
        transition.registry_revision().map(peritus_types::RevisionNumber::get),
    );
    match aggregate.resolution() {
        Some(resolution) => {
            encoding::u8_value(&mut payload, 1);
            encoding::digest(&mut payload, resolution.decision_digest().sha256());
            payload.extend_from_slice(resolution.command_id().as_bytes());
            encoding::u64_value(&mut payload, resolution.registry_revision().get());
            encoding::digest(&mut payload, resolution.registry_digest());
            encoding::u64_value(&mut payload, resolution.credential_generation().get());
            encoding::instant(&mut payload, resolution.valid_until());
        }
        None => encoding::u8_value(&mut payload, 0),
    }
    match registry {
        Some((revision, generation, digest)) => {
            encoding::u8_value(&mut payload, 1);
            encoding::u64_value(&mut payload, revision);
            encoding::u64_value(&mut payload, generation);
            encoding::digest(&mut payload, digest);
        }
        None => encoding::u8_value(&mut payload, 0),
    }
    encoding::value(VALUE_KIND, &payload)
}

const fn transition_kind_tag(kind: ApprovalTransitionKind) -> u8 {
    match kind {
        ApprovalTransitionKind::Resolved => 1,
        ApprovalTransitionKind::Idempotent => 2,
        ApprovalTransitionKind::Expired => 3,
        ApprovalTransitionKind::Cancelled => 4,
    }
}

const fn phase_tag(phase: ApprovalPhase) -> u8 {
    match phase {
        ApprovalPhase::Pending => 1,
        ApprovalPhase::ApprovedOnce => 2,
        ApprovalPhase::AmendmentAuthorized => 3,
        ApprovalPhase::Consumed => 4,
        ApprovalPhase::Amended => 5,
        ApprovalPhase::Denied => 6,
        ApprovalPhase::Expired => 7,
        ApprovalPhase::Cancelled => 8,
    }
}
