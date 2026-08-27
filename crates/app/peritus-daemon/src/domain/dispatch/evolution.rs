//! Family-88 campaign and family-91 production-pointer dispatch.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::SqliteJournal;

use super::{
    DomainOutcome, DomainSubmission, binding_matches_without_revision, binding_rejection,
    domain_failure, malformed, semantic_rejection,
};
use crate::DaemonError;

pub(super) fn dispatch_campaign(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_evolution::CampaignCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let replay = peritus_evolution::recover_campaign(journal, frame.campaign_id())
        .map_err(|error| domain_failure("load evolution campaign", error))?;
    let command = match frame.into_command() {
        Ok(command) => command,
        Err(_) => return semantic_rejection(),
    };
    if !binding_matches_without_revision(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_head(),
    ) || campaign_revision(replay.state(), command.kind()) != Some(submission.revision)
    {
        return binding_rejection();
    }
    let transition = match peritus_evolution::decide_campaign(replay.state(), &command) {
        Ok(transition) => transition,
        Err(_) => return semantic_rejection(),
    };
    peritus_evolution::commit_campaign_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit evolution campaign", error))
}

pub(super) fn dispatch_pointer(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_evolution::PointerCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let replay = peritus_evolution::recover_pointer(journal, frame.project_id())
        .map_err(|error| domain_failure("load production pointer", error))?;
    let command = match frame.into_command() {
        Ok(command) => command,
        Err(_) => return semantic_rejection(),
    };
    if !binding_matches_without_revision(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_head(),
    ) || pointer_revision(replay.state(), command.kind()) != Some(submission.revision)
    {
        return binding_rejection();
    }
    let transition = match peritus_evolution::decide_pointer(replay.state(), &command) {
        Ok(transition) => transition,
        Err(_) => return semantic_rejection(),
    };
    peritus_evolution::commit_pointer_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit production pointer", error))
}

fn campaign_revision(
    prior: Option<&peritus_evolution::CampaignState>,
    kind: &peritus_evolution::CampaignCommandKind,
) -> Option<peritus_types::RevisionTuple> {
    prior.map(|state| state.baseline().revision()).or_else(|| match kind {
        peritus_evolution::CampaignCommandKind::CreateCampaign { baseline, .. } => {
            Some(baseline.revision())
        }
        _ => None,
    })
}

fn pointer_revision(
    prior: Option<&peritus_evolution::ProductionHarnessState>,
    kind: &peritus_evolution::PointerCommandKind,
) -> Option<peritus_types::RevisionTuple> {
    prior.map(|state| state.current().revision()).or_else(|| match kind {
        peritus_evolution::PointerCommandKind::InitializeProductionHarness { initial, .. } => {
            Some(initial.revision())
        }
        _ => None,
    })
}
