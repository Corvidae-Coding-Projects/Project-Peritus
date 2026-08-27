//! Family-85 evaluation aggregate dispatch.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::SqliteJournal;

use super::{
    DomainOutcome, DomainSubmission, binding_matches_without_revision, binding_rejection,
    domain_failure, malformed, semantic_rejection,
};
use crate::DaemonError;

pub(super) fn dispatch(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_eval::EvaluationCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let replay = peritus_eval::load_evaluation_replay(journal, frame.campaign_id())
        .map_err(|error| domain_failure("load evaluation aggregate", error))?;
    let prior =
        replay.rebuild().map_err(|error| domain_failure("rebuild evaluation aggregate", error))?;
    let command = match frame.check() {
        Ok(command) => command,
        Err(_) => return semantic_rejection(),
    };
    if !binding_matches_without_revision(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
    ) || revision(prior.as_ref(), command.kind()) != Some(submission.revision)
    {
        return binding_rejection();
    }
    let transition = match peritus_eval::decide(prior.as_ref(), &command) {
        Ok(transition) => transition,
        Err(_) => return semantic_rejection(),
    };
    peritus_eval::commit_evaluation_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit evaluation transition", error))
}

fn revision(
    prior: Option<&peritus_eval::EvaluationState>,
    kind: &peritus_eval::EvaluationCommandKind,
) -> Option<peritus_types::RevisionTuple> {
    prior.map(|state| *state.revision()).or(match kind {
        peritus_eval::EvaluationCommandKind::CreateCampaign { revision, .. } => Some(*revision),
        _ => None,
    })
}
