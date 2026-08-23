//! Reducer-driven reconstruction of durable B0 aggregate chains.

use peritus_kernel::{KernelAggregate, KernelEvent};

use super::{
    KernelReplayCapsule, KernelReplayDriver, NAMESPACE, aggregate_key,
    capsule::{decode_capsule, exact, revision_digest},
    corrupt, input,
    state_digest::kernel_state_digest,
};
use crate::{AggregateKey, AggregateKind, JournalError, JournalErrorKind, SqliteJournal};

/// Move-only aggregate reconstructed by reducer replay from the complete durable chain.
pub struct RecoveredKernelAggregate {
    aggregate: KernelAggregate,
    batch_count: u64,
}

impl RecoveredKernelAggregate {
    /// Borrows the replay-checked current aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &KernelAggregate {
        &self.aggregate
    }

    /// Returns the number of replayed committed transitions.
    #[must_use]
    pub const fn transition_count(&self) -> u64 {
        self.batch_count
    }

    /// Consumes the observation into the reconstructed aggregate.
    #[must_use]
    pub fn into_aggregate(self) -> KernelAggregate {
        self.aggregate
    }
}

impl SqliteJournal {
    /// Reconstructs one B0 aggregate from genesis by invoking the verified reducers for every
    /// exact stored command/input capsule and comparing every emitted event and successor digest.
    ///
    /// # Errors
    ///
    /// Returns not-found, input-resolution, or terminal replay-integrity failures.
    pub fn recover_kernel<D: KernelReplayDriver>(
        &self,
        key: AggregateKey,
        driver: &mut D,
    ) -> Result<RecoveredKernelAggregate, JournalError> {
        if key.kind() != AggregateKind::Kernel {
            return Err(input("kernel recovery requires a kernel aggregate key"));
        }
        let records = self.records_for_aggregate(key)?;
        if records.is_empty() {
            return Err(JournalError::new(
                JournalErrorKind::NotFound,
                "recover kernel aggregate",
                "kernel aggregate has no durable genesis event",
            ));
        }
        let mut current = None;
        for (index, record) in records.iter().enumerate() {
            let state = self
                .state_record(NAMESPACE, record.event_id().as_bytes())?
                .ok_or_else(|| corrupt("kernel event has no atomic replay capsule"))?;
            if state.producing_position() != record.global_position() {
                return Err(corrupt("kernel replay capsule names another producing event"));
            }
            let capsule = decode_capsule(state.bytes())?;
            validate_capsule_record(&capsule, record)?;
            let (aggregate, event) = if index == 0 {
                if !capsule.is_genesis() {
                    return Err(corrupt("first kernel replay capsule is not genesis"));
                }
                let genesis = driver
                    .replay_genesis(&capsule)
                    .map_err(|_| input("kernel genesis replay input resolution failed"))?;
                genesis.into_parts()
            } else {
                if capsule.is_genesis() {
                    return Err(corrupt("non-genesis kernel event contains a genesis capsule"));
                }
                let before = current.take().ok_or_else(|| corrupt("kernel replay lost state"))?;
                let transition = driver
                    .replay_transition(before, &capsule)
                    .map_err(|_| input("kernel transition replay input resolution failed"))?;
                let (aggregate, event, _) = transition.into_parts();
                (aggregate, event)
            };
            verify_replayed(&aggregate, event, &capsule, record)?;
            current = Some(aggregate);
        }
        let aggregate = current.ok_or_else(|| corrupt("kernel replay produced no aggregate"))?;
        if aggregate_key(&aggregate)? != key {
            return Err(corrupt("replayed kernel aggregate identity changed"));
        }
        Ok(RecoveredKernelAggregate {
            aggregate,
            batch_count: u64::try_from(records.len())
                .map_err(|_| corrupt("kernel replay count overflowed"))?,
        })
    }
}

fn validate_capsule_record(
    capsule: &KernelReplayCapsule,
    record: &crate::CommittedRecord,
) -> Result<(), JournalError> {
    let envelope = capsule.envelope();
    let same_command = envelope.command_id() == record.command_id();
    let same_event = envelope.event_id() == record.event_id();
    let same_predecessor = envelope.expected_previous_event_id() == record.previous_event_id();
    let same_revision = revision_digest(envelope.revision()) == record.revision_digest();
    let envelope_matches_record = same_command && same_event && same_predecessor && same_revision;
    if !envelope_matches_record
        || capsule.session_id.as_bytes() != record.aggregate().id().as_bytes()
    {
        return Err(corrupt("kernel replay capsule does not match its journal record"));
    }
    Ok(())
}

fn verify_replayed(
    aggregate: &KernelAggregate,
    event: KernelEvent,
    capsule: &KernelReplayCapsule,
    record: &crate::CommittedRecord,
) -> Result<(), JournalError> {
    let frame =
        exact(&peritus_protocol::KernelEventDto::from(event), "encode replayed kernel event")?;
    if frame.bytes() != record.frame_bytes()
        || aggregate.head_event_id() != record.event_id()
        || aggregate.last_sequence() != record.sequence()
        || kernel_state_digest(aggregate) != capsule.successor_digest
    {
        return Err(corrupt("B0 reducer replay differs from committed event or successor state"));
    }
    Ok(())
}
