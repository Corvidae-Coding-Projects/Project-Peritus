//! Canonical append collection and hash-chain validation.

use std::collections::HashSet;

use peritus_types::{CommandId, EventId, Sha256Digest};

use super::{
    AppendRequest, HeadExpectation, MAX_ARTIFACT_DEPENDENCIES, MAX_BATCH_AGGREGATES,
    MAX_BATCH_EVENTS, MAX_OUTBOX_ENTRIES, MAX_STATE_INSTALLS, PlannedEvent,
};
use crate::{
    ArtifactDependency, EventDraft, JournalError, JournalErrorKind, OutboxDraft, StateInstall,
    hash_chain::event_hash,
};

pub(super) const fn validate_bounds(request: &AppendRequest) -> Result<(), JournalError> {
    if request.events.is_empty() {
        return Err(JournalError::new(
            JournalErrorKind::EmptyBatch,
            "plan append",
            "an append must contain at least one event",
        ));
    }
    if request.events.len() > MAX_BATCH_EVENTS
        || request.heads.is_empty()
        || request.heads.len() > MAX_BATCH_AGGREGATES
        || request.state_installs.len() > MAX_STATE_INSTALLS
        || request.outbox.len() > MAX_OUTBOX_ENTRIES
        || request.artifact_dependencies.len() > MAX_ARTIFACT_DEPENDENCIES
    {
        return Err(JournalError::new(
            JournalErrorKind::InvalidInput,
            "plan append",
            "append collection bound exceeded or no aggregate precondition supplied",
        ));
    }
    Ok(())
}

pub(super) fn validate_heads(heads: &[HeadExpectation]) -> Result<(), JournalError> {
    for pair in heads.windows(2) {
        if pair[0].key() == pair[1].key() {
            return Err(JournalError::new(
                JournalErrorKind::DuplicateIdentity,
                "plan append",
                "duplicate aggregate head precondition",
            ));
        }
        if pair[0].key() > pair[1].key() {
            return Err(JournalError::new(
                JournalErrorKind::NonCanonicalOrder,
                "plan append",
                "aggregate head preconditions must be strictly ordered",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_state_installs(installs: &[StateInstall]) -> Result<(), JournalError> {
    for pair in installs.windows(2) {
        let left = (pair[0].namespace(), pair[0].key());
        let right = (pair[1].namespace(), pair[1].key());
        if left == right {
            return Err(JournalError::new(
                JournalErrorKind::DuplicateIdentity,
                "plan append",
                "duplicate state install",
            ));
        }
        if left > right {
            return Err(JournalError::new(
                JournalErrorKind::NonCanonicalOrder,
                "plan append",
                "state installs must be strictly ordered",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_artifacts(dependencies: &[ArtifactDependency]) -> Result<(), JournalError> {
    for pair in dependencies.windows(2) {
        if pair[0] == pair[1] {
            return Err(JournalError::new(
                JournalErrorKind::DuplicateIdentity,
                "plan append",
                "duplicate artifact dependency",
            ));
        }
        if pair[0] > pair[1] {
            return Err(JournalError::new(
                JournalErrorKind::NonCanonicalOrder,
                "plan append",
                "artifact dependencies must be strictly ordered",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_outbox(outbox: &[OutboxDraft]) -> Result<(), JournalError> {
    for pair in outbox.windows(2) {
        if pair[0].id() == pair[1].id() {
            return Err(JournalError::new(
                JournalErrorKind::DuplicateIdentity,
                "plan append",
                "duplicate outbox identity",
            ));
        }
        if pair[0].id() > pair[1].id() {
            return Err(JournalError::new(
                JournalErrorKind::NonCanonicalOrder,
                "plan append",
                "outbox entries must be strictly ordered",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_and_hash_events(
    heads: &[HeadExpectation],
    events: Vec<EventDraft>,
    command_id: CommandId,
) -> Result<Vec<PlannedEvent>, JournalError> {
    let mut seen_ids = HashSet::with_capacity(events.len());
    let mut planned = Vec::with_capacity(events.len());
    let mut previous_key = None;
    let mut chain_head: Option<(u64, EventId, Sha256Digest)> = None;

    for draft in events {
        if !seen_ids.insert(draft.event_id()) {
            return Err(JournalError::new(
                JournalErrorKind::DuplicateIdentity,
                "plan append",
                "duplicate event identity",
            ));
        }
        if previous_key.is_some_and(|key| key > draft.aggregate()) {
            return Err(JournalError::new(
                JournalErrorKind::NonCanonicalOrder,
                "plan append",
                "events must be ordered by aggregate and sequence",
            ));
        }
        if previous_key != Some(draft.aggregate()) {
            let expectation = heads
                .binary_search_by_key(&draft.aggregate(), |head| head.key())
                .ok()
                .map(|index| heads[index])
                .ok_or_else(|| {
                    JournalError::new(
                        JournalErrorKind::InvalidInput,
                        "plan append",
                        "event has no aggregate head precondition",
                    )
                })?;
            chain_head = expectation
                .observed()
                .map(|head| (head.sequence().get(), head.event_id(), head.event_hash()));
            previous_key = Some(draft.aggregate());
        }
        let (expected_sequence, expected_id, previous_hash) = match chain_head {
            Some((sequence, event_id, hash)) => (
                sequence.checked_add(1).ok_or_else(|| {
                    JournalError::new(
                        JournalErrorKind::SequenceOverflow,
                        "plan append",
                        "aggregate sequence exhausted",
                    )
                })?,
                Some(event_id),
                hash,
            ),
            None => (1, None, Sha256Digest::new([0; 32])),
        };
        if !crate::verified::extends_sequence(
            chain_head.map(|(sequence, _, _)| sequence),
            draft.sequence().get(),
        ) || draft.sequence().get() != expected_sequence
            || draft.previous_event_id() != expected_id
        {
            return Err(JournalError::new(
                JournalErrorKind::StaleHead,
                "plan append",
                "event sequence or predecessor does not extend its declared head",
            ));
        }
        let hash = event_hash(&draft, previous_hash, command_id);
        chain_head = Some((draft.sequence().get(), draft.event_id(), hash));
        planned.push(PlannedEvent { draft, previous_hash, event_hash: hash });
    }

    for head in heads {
        if !planned.iter().any(|event| event.draft.aggregate() == head.key()) {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "plan append",
                "head precondition has no event in the batch",
            ));
        }
    }
    Ok(planned)
}
