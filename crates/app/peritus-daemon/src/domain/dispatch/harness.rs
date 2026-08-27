//! Family-79 harness aggregate dispatch.

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
    let frame = match decode_message::<peritus_harness::HarnessCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let replay = peritus_harness::load_harness_replay(journal, frame.harness_id())
        .map_err(|error| domain_failure("load harness aggregate", error))?;
    let prior =
        replay.rebuild().map_err(|error| domain_failure("rebuild harness aggregate", error))?;
    let command = match frame.check(prior.as_ref()) {
        Ok(command) => command,
        Err(_) => return semantic_rejection(),
    };
    if !binding_matches_without_revision(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
    ) {
        return binding_rejection();
    }
    let transition = match peritus_harness::decide(prior.as_ref(), &command) {
        Ok(transition) => transition,
        Err(_) => return semantic_rejection(),
    };
    peritus_harness::commit_harness_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit harness transition", error))
}
