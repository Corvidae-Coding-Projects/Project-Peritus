//! Stable outbox planning and finalized-artifact dependencies.

use peritus_journal::{ArtifactDependency, OutboxDraft};

use crate::{
    DebuggerCommand, DebuggerCommandKind, DebuggerError, DebuggerEventKind, DebuggerState,
};

use super::super::{
    MODEL_ANALYSIS_DESTINATION, ModelDirective, PUBLICATION_DESTINATION, PublicationDirective,
    binding,
};

const OUTBOX_MAX_DELIVERY_ATTEMPTS: u16 = 16;

pub(super) fn transition_outbox(
    command: &DebuggerCommand,
    state: &DebuggerState,
) -> Result<Vec<OutboxDraft>, DebuggerError> {
    let draft = match command.kind() {
        DebuggerCommandKind::RequestModelAnalysis {
            model_id, plan_digest, request_digest, ..
        } => {
            let directive = ModelDirective::new(
                command.job_id(),
                *model_id,
                1,
                *plan_digest,
                *request_digest,
                0,
            )?;
            Some(outbox_draft(
                directive.outbox_id()?,
                MODEL_ANALYSIS_DESTINATION,
                directive.canonical_bytes()?,
            ))
        }
        DebuggerCommandKind::ScheduleModelRetry { model_id, next_attempt, not_before_tick } => {
            let model = state
                .model()
                .ok_or_else(|| binding::binding("retry successor has no frozen model state"))?;
            let directive = ModelDirective::new(
                command.job_id(),
                *model_id,
                *next_attempt,
                model.plan_digest(),
                model.request_digest(),
                *not_before_tick,
            )?;
            Some(outbox_draft(
                directive.outbox_id()?,
                MODEL_ANALYSIS_DESTINATION,
                directive.canonical_bytes()?,
            ))
        }
        DebuggerCommandKind::CompleteReport { report } => {
            let directive = PublicationDirective::new(command.job_id(), *report);
            Some(outbox_draft(
                directive.outbox_id()?,
                PUBLICATION_DESTINATION,
                directive.canonical_bytes()?,
            ))
        }
        _ => None,
    };
    draft.map_or_else(|| Ok(Vec::new()), |value| value.map(|entry| vec![entry]))
}

pub(super) fn artifact_dependencies(kind: &DebuggerEventKind) -> Vec<ArtifactDependency> {
    match kind {
        DebuggerEventKind::ReportCompleted { report } => {
            vec![ArtifactDependency::new(report.digest())]
        }
        DebuggerEventKind::PublicationRecorded { publication } => {
            vec![ArtifactDependency::new(publication.artifact_digest())]
        }
        _ => Vec::new(),
    }
}

fn outbox_draft(
    id: peritus_journal::OutboxId,
    destination: &str,
    payload: Vec<u8>,
) -> Result<OutboxDraft, DebuggerError> {
    OutboxDraft::new(id, destination.to_owned(), payload, OUTBOX_MAX_DELIVERY_ATTEMPTS)
        .map_err(binding::journal)
}
