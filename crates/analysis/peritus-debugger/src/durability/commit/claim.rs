//! Commit-mode validation and claim-bound idempotency digests.

use peritus_journal::OutboxAcknowledgement;

use crate::{DebuggerCommand, DebuggerCommandKind, DebuggerError, DebuggerState, ModelWorkState};

use super::{CommitMode, DebuggerDirectiveClaim, binding};

pub(super) fn validate_mode(
    command: &DebuggerCommand,
    state: &DebuggerState,
    mode: CommitMode,
) -> Result<(), DebuggerError> {
    match mode {
        CommitMode::Ordinary => match command.kind() {
            DebuggerCommandKind::MarkModelAttemptStarted { .. }
            | DebuggerCommandKind::RecordModelProposal { .. }
            | DebuggerCommandKind::RecordModelFailure { .. }
            | DebuggerCommandKind::RecordPublication { .. } => {
                Err(binding::binding("effect transition requires its exact claimed directive"))
            }
            _ => Ok(()),
        },
        CommitMode::Claimed(DebuggerDirectiveClaim::Model(claim)) => {
            let directive = claim.directive();
            let DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt, started_at_tick } =
                command.kind()
            else {
                return Err(binding::binding("claim-fenced commit is not a model-attempt start"));
            };
            let model = state.model().ok_or_else(|| {
                binding::binding("model-attempt start successor has no model state")
            })?;
            if directive.job_id() != command.job_id()
                || directive.model_id() != *model_id
                || directive.attempt() != *attempt
                || directive.plan_digest() != model.plan_digest()
                || directive.request_digest() != model.request_digest()
                || *started_at_tick < directive.not_before_tick()
                || !matches!(
                    model.state(),
                    ModelWorkState::Running {
                        attempt: current,
                        ..
                    } if current == *attempt
                )
            {
                return Err(binding::binding("claimed model directive differs from attempt start"));
            }
            Ok(())
        }
        CommitMode::Claimed(DebuggerDirectiveClaim::Publication(_)) => {
            Err(binding::binding("publication claims have no pre-effect aggregate transition"))
        }
        CommitMode::Settlement(claim) => validate_settlement(command, claim),
    }
}

pub(super) fn acknowledgement(
    claim: DebuggerDirectiveClaim,
) -> Result<OutboxAcknowledgement, DebuggerError> {
    OutboxAcknowledgement::new(claim.id()?, claim.fence()).map_err(binding::journal)
}

pub(super) fn claimed_digest(
    base: peritus_types::Sha256Digest,
    claim: DebuggerDirectiveClaim,
) -> Result<peritus_types::Sha256Digest, DebuggerError> {
    bound_digest(b"PERITUS-E2-OUTBOX-CLAIM\0", base, claim)
}

pub(super) fn acknowledged_digest(
    base: peritus_types::Sha256Digest,
    claim: DebuggerDirectiveClaim,
) -> Result<peritus_types::Sha256Digest, DebuggerError> {
    bound_digest(b"PERITUS-C0-OUTBOX-ACKNOWLEDGEMENTS\0", base, claim)
}

fn validate_settlement(
    command: &DebuggerCommand,
    claim: DebuggerDirectiveClaim,
) -> Result<(), DebuggerError> {
    let matches = match (command.kind(), claim) {
        (
            DebuggerCommandKind::RecordModelProposal { model_id, attempt, .. },
            DebuggerDirectiveClaim::Model(value),
        ) => {
            value.directive().job_id() == command.job_id()
                && value.directive().model_id() == *model_id
                && value.directive().attempt() == *attempt
        }
        (
            DebuggerCommandKind::RecordModelFailure { failure },
            DebuggerDirectiveClaim::Model(value),
        ) => {
            value.directive().job_id() == command.job_id()
                && value.directive().model_id() == failure.model_id()
                && value.directive().attempt() == failure.attempt()
        }
        (
            DebuggerCommandKind::RecordPublication { publication },
            DebuggerDirectiveClaim::Publication(value),
        ) => {
            value.directive().job_id() == command.job_id()
                && value.directive().report().id() == publication.report_id()
                && value.directive().report().digest() == publication.artifact_digest()
                && value.directive().report().size() == publication.artifact_size()
        }
        (
            DebuggerCommandKind::CancelJob { .. } | DebuggerCommandKind::FailJob { .. },
            DebuggerDirectiveClaim::Model(value),
        ) => value.directive().job_id() == command.job_id(),
        (
            DebuggerCommandKind::CancelJob { .. } | DebuggerCommandKind::FailJob { .. },
            DebuggerDirectiveClaim::Publication(value),
        ) => value.directive().job_id() == command.job_id(),
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(binding::binding("directive claim does not match the effect settlement"))
    }
}

fn bound_digest(
    domain: &[u8],
    base: peritus_types::Sha256Digest,
    claim: DebuggerDirectiveClaim,
) -> Result<peritus_types::Sha256Digest, DebuggerError> {
    let id = claim.id()?;
    let mut bytes = Vec::with_capacity(domain.len() + 32 + 8 + 16 + 8);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(base.as_bytes());
    bytes.extend_from_slice(&1_u64.to_be_bytes());
    bytes.extend_from_slice(id.as_bytes());
    bytes.extend_from_slice(&claim.fence().to_be_bytes());
    Ok(peritus_codec::sha256(&bytes))
}
