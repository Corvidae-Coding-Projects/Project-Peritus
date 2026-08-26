//! Cross-record harness transition binding validation.

use peritus_journal::{AggregateId, AggregateKey, AggregateKind};
use peritus_types::HarnessId;

use crate::aggregate::{HarnessCommand, HarnessCommandKind, HarnessEventKind, HarnessTransition};

use super::{DurabilityError, DurabilityErrorKind, DurabilityRecovery};

/// Journal-owned namespace for complete current harness checkpoints.
pub const HARNESS_STATE_NAMESPACE: u16 = 0xE101;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.harness.state-key.v1\0";

/// Derives the dedicated C0 harness aggregate key.
///
/// # Errors
/// Rejects a harness identity C0 cannot represent.
pub fn harness_aggregate_key(harness_id: HarnessId) -> Result<AggregateKey, DurabilityError> {
    let id = AggregateId::new(*harness_id.as_bytes()).map_err(|error| {
        DurabilityError::new(
            DurabilityErrorKind::Binding,
            DurabilityRecovery::CorrectInput,
            error.to_string(),
        )
    })?;
    Ok(AggregateKey::new(AggregateKind::Harness, id))
}

/// Derives the stable domain-separated complete-checkpoint key.
#[must_use]
pub fn harness_state_key(harness_id: HarnessId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + 16);
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(harness_id.as_bytes());
    key
}

pub(super) fn validate(
    command: &HarnessCommand,
    transition: &HarnessTransition,
) -> Result<(), DurabilityError> {
    let event = transition.event();
    let state = transition.state();
    let mismatched = [
        command.event_id() != event.id(),
        command.command_id() != event.command_id(),
        command.harness_id() != event.harness_id(),
        command.harness_id() != state.harness_id(),
        command.expected_previous_event() != event.previous_event(),
        command.expected_sequence().checked_add(1) != Some(event.sequence()),
        command.prior_state_digest() != event.prior_state_digest(),
        command.digest() != event.command_digest(),
        event.successor_state_digest() != state.state_digest(),
        event.sequence() != state.sequence(),
        event.id() != state.last_event_id(),
        !event_matches_command(command.kind(), event.kind()),
    ]
    .into_iter()
    .any(core::convert::identity);
    if mismatched {
        return Err(binding("command, event, and successor checkpoint bindings differ"));
    }
    let revision = state
        .history()
        .revision(event.revision_digest())
        .ok_or_else(|| binding("event revision is absent from successor history"))?;
    let mut roots = state_roots_for_revision(revision);
    roots.sort_unstable();
    roots.dedup();
    if roots != event.artifact_roots() {
        return Err(binding("event artifact roots differ from its bound revision"));
    }
    Ok(())
}

fn state_roots_for_revision(
    revision: &crate::domain::HarnessRevision,
) -> Vec<peritus_types::Sha256Digest> {
    let mut roots = Vec::new();
    for root in revision.artifact_roots() {
        roots.push(root.content_digest());
        if let Some(executable) = root.executable_artifact_digest() {
            roots.push(executable.digest());
        }
    }
    roots
}

fn event_matches_command(command: &HarnessCommandKind, event: &HarnessEventKind) -> bool {
    match (command, event) {
        (
            HarnessCommandKind::RegisterGenesis { revision: left },
            HarnessEventKind::GenesisRegistered { revision: right },
        )
        | (
            HarnessCommandKind::RegisterSuccessor { revision: left },
            HarnessEventKind::SuccessorRegistered { revision: right },
        ) => left == right,
        (
            HarnessCommandKind::PlanMaterialization { plan: left },
            HarnessEventKind::MaterializationPlanned { plan: right },
        ) => left == right,
        (
            HarnessCommandKind::AcknowledgeDirectiveDelivery {
                plan_id: left_id,
                delivered_at_millis: left_time,
            },
            HarnessEventKind::DirectiveDeliveryAcknowledged {
                plan_id: right_id,
                delivered_at_millis: right_time,
            },
        ) => left_id == right_id && left_time == right_time,
        (
            HarnessCommandKind::RecordMaterialization { receipt: left },
            HarnessEventKind::MaterializationRecorded { receipt: right },
        ) => left == right,
        (
            HarnessCommandKind::RecordMaterializationFailure { failure: left },
            HarnessEventKind::MaterializationFailureRecorded { failure: right },
        ) => left == right,
        (
            HarnessCommandKind::ReconcilePendingMaterialization {
                plan_id: left_id,
                decision: left,
            },
            HarnessEventKind::PendingMaterializationReconciled {
                plan_id: right_id,
                decision: right,
            },
        ) => left_id == right_id && left == right,
        (
            HarnessCommandKind::RetireSettledReceipt { receipt_id: left },
            HarnessEventKind::SettledReceiptRetired { receipt_id: right },
        ) => left == right,
        _ => false,
    }
}

pub(super) fn binding(detail: &'static str) -> DurabilityError {
    DurabilityError::new(DurabilityErrorKind::Binding, DurabilityRecovery::Quarantine, detail)
}
