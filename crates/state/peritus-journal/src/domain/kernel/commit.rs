//! Move-only B0 commit requests and post-commit observations.

use peritus_kernel::{
    AcceptanceOutcome, CommandEnvelope, KernelAggregate, KernelCommand, KernelEvent, KernelGenesis,
    KernelTransition,
};
use peritus_protocol::{CommandEnvelopeDto, KernelCommandDto, KernelEventDto};
use peritus_types::Sha256Digest;

use super::{
    CapsuleKind, DOMAIN, KernelInputReference, KernelReplayCapsule, NAMESPACE, aggregate_key,
    input, state_digest::kernel_state_digest, validate_inputs,
};
use crate::{
    AppendRequest, CommittedBatch, JournalError, SqliteJournal, StateInstall, domain::commit,
};

use super::capsule::{encode_capsule, exact, revision_digest};

/// Move-only request consuming one accepted B0 successor until commit succeeds.
pub struct KernelCommitRequest {
    append: AppendRequest,
    aggregate: KernelAggregate,
    event: KernelEvent,
    acceptance: Option<AcceptanceOutcome>,
    install: StateInstall,
}

impl KernelCommitRequest {
    /// Binds a session-open result to the exact event, envelope, inputs, and successor digest.
    ///
    /// # Errors
    ///
    /// Returns a canonical input error when the append does not contain exactly the genesis event.
    pub fn genesis(
        append: AppendRequest,
        genesis: KernelGenesis,
        envelope: CommandEnvelope,
        inputs: Vec<KernelInputReference>,
    ) -> Result<Self, JournalError> {
        let (aggregate, event) = genesis.into_parts();
        Self::build(append, aggregate, event, None, envelope, None, inputs, CapsuleKind::Genesis)
    }

    /// Binds an accepted reducer transition to the exact command/envelope/input replay capsule.
    ///
    /// # Errors
    ///
    /// Returns a canonical input error when identities, ordering, or event bytes disagree.
    pub fn transition(
        append: AppendRequest,
        transition: KernelTransition,
        envelope: CommandEnvelope,
        command: KernelCommand,
        inputs: Vec<KernelInputReference>,
    ) -> Result<Self, JournalError> {
        let (aggregate, event, acceptance) = transition.into_parts();
        Self::build(
            append,
            aggregate,
            event,
            acceptance,
            envelope,
            Some(command),
            inputs,
            CapsuleKind::Transition,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        append: AppendRequest,
        aggregate: KernelAggregate,
        event: KernelEvent,
        acceptance: Option<AcceptanceOutcome>,
        envelope: CommandEnvelope,
        command: Option<KernelCommand>,
        inputs: Vec<KernelInputReference>,
        kind: CapsuleKind,
    ) -> Result<Self, JournalError> {
        validate_inputs(&inputs)?;
        let same_command = envelope.command_id() == event.command_id();
        let same_event = envelope.event_id() == event.id();
        let same_predecessor = envelope.expected_previous_event_id() == event.previous_event_id();
        let same_revision = envelope.revision() == event.revision();
        let envelope_matches_event =
            same_command && same_event && same_predecessor && same_revision;
        let aggregate_matches_event = aggregate.head_event_id() == event.id()
            && aggregate.last_sequence() == event.sequence()
            && aggregate.revision() == event.revision();
        if !envelope_matches_event || !aggregate_matches_event {
            return Err(input("B0 successor, event, and envelope do not match exactly"));
        }
        let event_frame = exact(&KernelEventDto::from(event), "encode kernel event")?;
        let revision_digest = revision_digest(event.revision());
        let aggregate_key = aggregate_key(&aggregate)?;
        append.validate_single_kernel_event(aggregate_key, event, &event_frame, revision_digest)?;
        let envelope_frame = exact(&CommandEnvelopeDto::from(envelope), "encode kernel envelope")?;
        let command_frame = command
            .as_ref()
            .map(|command| exact(&KernelCommandDto::from(command.clone()), "encode kernel command"))
            .transpose()?;
        if matches!(kind, CapsuleKind::Genesis) != command_frame.is_none() {
            return Err(input("genesis and command capsule kind disagree"));
        }
        let capsule = KernelReplayCapsule {
            kind,
            project_id: aggregate.project_id(),
            session_id: aggregate.session().id(),
            envelope,
            envelope_frame,
            command,
            command_frame,
            inputs,
            successor_digest: kernel_state_digest(&aggregate),
        };
        let install = StateInstall::new(
            NAMESPACE,
            event.id().as_bytes().to_vec(),
            None,
            1,
            encode_capsule(&capsule),
        )?;
        Ok(Self { append, aggregate, event, acceptance, install })
    }
}

/// Opaque receipt exposing B0's next aggregate only after its exact durable append is observed.
pub struct CommittedKernelTransition {
    batch: CommittedBatch,
    aggregate: KernelAggregate,
    acceptance: Option<AcceptanceOutcome>,
    state_digest: Sha256Digest,
}

impl CommittedKernelTransition {
    /// Borrows the exact committed event batch.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }

    /// Borrows the exact B0 successor made available after commit.
    #[must_use]
    pub const fn aggregate(&self) -> &KernelAggregate {
        &self.aggregate
    }

    /// Returns the reducer's acceptance result when this was an evaluation command.
    #[must_use]
    pub const fn acceptance_outcome(&self) -> Option<AcceptanceOutcome> {
        self.acceptance
    }

    /// Returns the canonical successor-state digest persisted in the replay capsule.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }

    /// Consumes the receipt into the durable batch and exact successor aggregate.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, KernelAggregate) {
        (self.batch, self.aggregate)
    }
}

impl SqliteJournal {
    /// Atomically commits one accepted B0 transition and returns its next aggregate only afterward.
    ///
    /// # Errors
    ///
    /// Returns journal CAS, idempotency, storage, or exact post-commit observation failures.
    pub fn commit_kernel_transition(
        &mut self,
        request: KernelCommitRequest,
    ) -> Result<CommittedKernelTransition, JournalError> {
        let KernelCommitRequest { append, aggregate, event, acceptance, install } = request;
        let state_digest = install.digest();
        let (batch, _) = commit::commit_state(self, append, DOMAIN, install)?;
        if batch.records().len() != 1 || batch.records()[0].event_id() != event.id() {
            return Err(super::corrupt("committed batch does not contain the exact B0 event"));
        }
        Ok(CommittedKernelTransition { batch, aggregate, acceptance, state_digest })
    }
}
