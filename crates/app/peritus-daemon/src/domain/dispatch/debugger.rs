//! Family-82 debugger aggregate dispatch.

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
    let frame = match decode_message::<peritus_debugger::DebuggerCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let replay = peritus_debugger::load_debugger_replay(journal, frame.job_id())
        .map_err(|error| domain_failure("load debugger aggregate", error))?;
    let prior =
        replay.rebuild().map_err(|error| domain_failure("rebuild debugger aggregate", error))?;
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
    let transition = match peritus_debugger::decide(prior.as_ref(), &command) {
        Ok(transition) => transition,
        Err(_) => return semantic_rejection(),
    };
    peritus_debugger::commit_debugger_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit debugger transition", error))
}

fn revision(
    prior: Option<&peritus_debugger::DebuggerState>,
    kind: &peritus_debugger::DebuggerCommandKind,
) -> Option<peritus_types::RevisionTuple> {
    prior.map(|state| *state.revision()).or_else(|| match kind {
        peritus_debugger::DebuggerCommandKind::CreateJob { revision, .. } => Some(*revision),
        _ => None,
    })
}
