//! Durable activation origins reconstructed from immutable C0 history.

use peritus_journal::StoreId;
use peritus_types::{CommandId, EventId, ProjectId, Sha256Digest};

use crate::{
    ActivationId, ActivationKind, ActivationRecord, EvolutionError, EvolutionErrorKind,
    EvolutionOperation, EvolutionRecovery, ProductionHarnessState, RollbackProposal,
    identity::digest_parts,
};

/// Exact immutable C0 origin of one production-pointer activation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableActivationOrigin {
    store_id: StoreId,
    project_id: ProjectId,
    activation: ActivationRecord,
    event_sequence: u64,
    event_id: EventId,
    command_id: CommandId,
    global_position: u64,
    previous_event_hash: Sha256Digest,
    event_hash: Sha256Digest,
    event_frame_digest: Sha256Digest,
    event_revision_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    checkpoint_digest: Sha256Digest,
    checkpoint_position: u64,
    digest: Sha256Digest,
}

impl DurableActivationOrigin {
    #[allow(
        clippy::too_many_arguments,
        reason = "every immutable event and checkpoint provenance fact stays explicit"
    )]
    pub(crate) fn from_committed(
        store_id: StoreId,
        project_id: ProjectId,
        activation: ActivationRecord,
        event_sequence: u64,
        event_id: EventId,
        command_id: CommandId,
        global_position: u64,
        previous_event_hash: Sha256Digest,
        event_hash: Sha256Digest,
        event_frame_digest: Sha256Digest,
        event_revision_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        checkpoint_digest: Sha256Digest,
        checkpoint_position: u64,
    ) -> Result<Self, EvolutionError> {
        if event_sequence == 0
            || global_position == 0
            || checkpoint_position < global_position
            || activation.generation() == 0
            || activation.generation() > event_sequence
        {
            return Err(corrupt("activation origin has impossible event or checkpoint provenance"));
        }
        let event_sequence_bytes = event_sequence.to_be_bytes();
        let global_position_bytes = global_position.to_be_bytes();
        let checkpoint_position_bytes = checkpoint_position.to_be_bytes();
        let digest = digest_parts(
            b"peritus.f0.durable-activation-origin.v1\0",
            &[
                store_id.as_bytes(),
                project_id.as_bytes(),
                activation.digest().as_bytes(),
                &event_sequence_bytes,
                event_id.as_bytes(),
                command_id.as_bytes(),
                &global_position_bytes,
                previous_event_hash.as_bytes(),
                event_hash.as_bytes(),
                event_frame_digest.as_bytes(),
                event_revision_digest.as_bytes(),
                successor_state_digest.as_bytes(),
                checkpoint_digest.as_bytes(),
                &checkpoint_position_bytes,
            ],
        );
        Ok(Self {
            store_id,
            project_id,
            activation,
            event_sequence,
            event_id,
            command_id,
            global_position,
            previous_event_hash,
            event_hash,
            event_frame_digest,
            event_revision_digest,
            successor_state_digest,
            checkpoint_digest,
            checkpoint_position,
            digest,
        })
    }

    /// Journal store that owns the immutable origin.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Project whose pointer chain contains the activation.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Complete content-derived activation record.
    #[must_use]
    pub const fn activation(&self) -> &ActivationRecord {
        &self.activation
    }
    /// Pointer aggregate sequence of the producing event.
    #[must_use]
    pub const fn event_sequence(&self) -> u64 {
        self.event_sequence
    }
    /// Producing pointer event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Producing command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Store-wide position of the producing pointer event.
    #[must_use]
    pub const fn global_position(&self) -> u64 {
        self.global_position
    }
    /// Hash of the exact preceding event in the C0 hash chain.
    #[must_use]
    pub const fn previous_event_hash(&self) -> Sha256Digest {
        self.previous_event_hash
    }
    /// Hash of the exact committed pointer event.
    #[must_use]
    pub const fn event_hash(&self) -> Sha256Digest {
        self.event_hash
    }
    /// Digest of the complete canonical pointer-event frame.
    #[must_use]
    pub const fn event_frame_digest(&self) -> Sha256Digest {
        self.event_frame_digest
    }
    /// Revision digest bound to the committed pointer event.
    #[must_use]
    pub const fn event_revision_digest(&self) -> Sha256Digest {
        self.event_revision_digest
    }
    /// Complete successor pointer-state digest declared by the event.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Digest of the exact inline or paged checkpoint root.
    #[must_use]
    pub const fn checkpoint_digest(&self) -> Sha256Digest {
        self.checkpoint_digest
    }
    /// Store-wide event position that installed the checkpoint revision.
    #[must_use]
    pub const fn checkpoint_position(&self) -> u64 {
        self.checkpoint_position
    }
    /// Digest of the complete activation and immutable origin proof.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Complete activation ledger derived from immutable events and historical checkpoints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableActivationHistory {
    store_id: StoreId,
    project_id: ProjectId,
    origins: Vec<DurableActivationOrigin>,
    digest: Sha256Digest,
}

impl DurableActivationHistory {
    pub(crate) fn empty(store_id: StoreId, project_id: ProjectId) -> Self {
        let digest = history_digest(store_id, project_id, &[]);
        Self { store_id, project_id, origins: Vec::new(), digest }
    }

    pub(crate) fn push(
        &mut self,
        origin: DurableActivationOrigin,
    ) -> Result<(), EvolutionError> {
        if origin.store_id != self.store_id || origin.project_id != self.project_id {
            return Err(corrupt("activation origin belongs to another durable pointer"));
        }
        let activation = origin.activation();
        let valid_successor = match self.origins.last() {
            None => {
                activation.kind() == ActivationKind::Initialization
                    && activation.generation() == 1
                    && activation.predecessor().is_none()
                    && origin.event_sequence == 1
            }
            Some(previous) => {
                let previous_activation = previous.activation();
                activation.kind() != ActivationKind::Initialization
                    && previous_activation.generation().checked_add(1)
                        == Some(activation.generation())
                    && activation.predecessor() == Some(previous_activation.successor())
                    && origin.event_sequence > previous.event_sequence
                    && origin.global_position > previous.global_position
                    && origin.checkpoint_position > previous.checkpoint_position
            }
        };
        let duplicate_id = self
            .origins
            .iter()
            .any(|value| value.activation.id() == activation.id());
        let reused_approval = activation.authorization().is_some_and(|authorization| {
            self.contains_approval_use(authorization.approval_use_digest())
        });
        if !valid_successor || duplicate_id || reused_approval {
            return Err(corrupt(
                "activation history is discontinuous, duplicated, or reuses authority",
            ));
        }
        self.digest = history_step(self.digest, origin.digest());
        self.origins.push(origin);
        Ok(())
    }

    pub(crate) fn matches_state(&self, state: &ProductionHarnessState) -> bool {
        if state.project_id() != self.project_id || self.origins.is_empty() {
            return false;
        }
        let Some(initial) = self.origins.first().map(DurableActivationOrigin::activation) else {
            return false;
        };
        let Some(latest) = self.origins.last().map(DurableActivationOrigin::activation) else {
            return false;
        };
        if initial.successor().harness_revision() != state.policy().production_revision()
            || latest.generation() != state.generation()
            || latest.successor() != state.current()
            || state.history().len() > self.origins.len()
        {
            return false;
        }
        let offset = self.origins.len() - state.history().len();
        state
            .history()
            .iter()
            .zip(&self.origins[offset..])
            .all(|(cached, durable)| cached == durable.activation())
    }

    pub(crate) fn validates_proposal(&self, proposal: &RollbackProposal) -> bool {
        proposal.project_id() == self.project_id
            && self.origin(proposal.target_activation()).is_some_and(|origin| {
                origin.activation().successor() == proposal.target()
                    && origin.activation().id() != proposal.rollback_of()
            })
    }

    pub(crate) fn contains_approval_use(&self, digest: Sha256Digest) -> bool {
        self.origins.iter().any(|origin| {
            origin
                .activation()
                .authorization()
                .is_some_and(|authorization| authorization.approval_use_digest() == digest)
        })
    }

    /// Journal store that owns this derived history.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Project whose pointer events were reconstructed.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Complete ordered immutable activation origins.
    #[must_use]
    pub fn origins(&self) -> &[DurableActivationOrigin] {
        &self.origins
    }
    /// Resolves one exact activation identity without consulting the checkpoint cache.
    #[must_use]
    pub fn origin(&self, activation_id: ActivationId) -> Option<&DurableActivationOrigin> {
        self.origins
            .iter()
            .find(|origin| origin.activation().id() == activation_id)
    }
    /// Digest of the complete ordered origin ledger.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn history_digest(
    store_id: StoreId,
    project_id: ProjectId,
    origins: &[DurableActivationOrigin],
) -> Sha256Digest {
    let mut digest = digest_parts(
        b"peritus.f0.durable-activation-history-root.v1\0",
        &[store_id.as_bytes(), project_id.as_bytes()],
    );
    for origin in origins {
        digest = history_step(digest, origin.digest());
    }
    digest
}

fn history_step(prior: Sha256Digest, origin: Sha256Digest) -> Sha256Digest {
    digest_parts(
        b"peritus.f0.durable-activation-history-step.v1\0",
        &[prior.as_bytes(), origin.as_bytes()],
    )
}

const fn corrupt(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::Recover,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
