//! Executable audit of the safety-critical reducer refinement boundary.

use crate::{
    AcceptancePhase, KernelAggregate, KernelEventKind, KernelSubject, RunPhase, SessionPhase,
};

#[doc = " Checks the concrete session and acceptance edges constrained by the closed proof model."]
#[allow(
    clippy::match_like_matches_macro,
    clippy::single_match,
    clippy::too_many_lines,
    clippy::unnested_or_patterns,
    reason = "explicit branches expose lifecycle facts to Verus without splitting the audited relation"
)]
pub(super) fn critical_step_is_legal(
    before: &KernelAggregate,
    after: &KernelAggregate,
    event: KernelEventKind,
    subject: KernelSubject,
) -> bool {
    match (event, subject) {
        (KernelEventKind::SessionPaused, KernelSubject::Session(id)) => {
            if before.session.id != id || after.session.id != id {
                return false;
            }
            match before.session.phase {
                SessionPhase::Open => {}
                _ => return false,
            }
            match after.session.phase {
                SessionPhase::Paused => true,
                _ => false,
            }
        }
        (KernelEventKind::SessionResumed, KernelSubject::Session(id)) => {
            if before.session.id != id || after.session.id != id {
                return false;
            }
            match before.session.phase {
                SessionPhase::Paused => {}
                _ => return false,
            }
            match after.session.phase {
                SessionPhase::Open => true,
                _ => false,
            }
        }
        (KernelEventKind::SessionClosed, KernelSubject::Session(id)) => {
            if before.session.id != id || after.session.id != id {
                return false;
            }
            match before.session.phase {
                SessionPhase::Closed => return false,
                _ => {}
            }
            match after.session.phase {
                SessionPhase::Closed => true,
                _ => false,
            }
        }
        (KernelEventKind::AcceptanceAccepted, KernelSubject::Acceptance(id)) => {
            let Some(before_index) = before.run_index(id) else {
                return false;
            };
            let Some(after_index) = after.run_index(id) else {
                return false;
            };
            let before_run = &before.runs[before_index];
            let after_run = &after.runs[after_index];
            match before_run.phase {
                RunPhase::Reviewing => {}
                _ => return false,
            }
            match before_run.acceptance {
                AcceptancePhase::Evaluating => {}
                _ => return false,
            }
            match after_run.phase {
                RunPhase::Accepted => {}
                _ => return false,
            }
            match after_run.acceptance {
                AcceptancePhase::Accepted => {}
                _ => return false,
            }
            true
        }
        (KernelEventKind::AcceptanceNeedsChanges, KernelSubject::Acceptance(id)) => {
            let Some(before_index) = before.run_index(id) else {
                return false;
            };
            let Some(after_index) = after.run_index(id) else {
                return false;
            };
            let before_run = &before.runs[before_index];
            let after_run = &after.runs[after_index];
            match before_run.phase {
                RunPhase::Reviewing => {}
                _ => return false,
            }
            match before_run.acceptance {
                AcceptancePhase::Evaluating => {}
                _ => return false,
            }
            match after_run.phase {
                RunPhase::Fixing => {}
                _ => return false,
            }
            match after_run.acceptance {
                AcceptancePhase::NeedsChanges => {}
                _ => return false,
            }
            true
        }
        (KernelEventKind::SessionPaused, _)
        | (KernelEventKind::SessionResumed, _)
        | (KernelEventKind::SessionClosed, _)
        | (KernelEventKind::AcceptanceAccepted, _)
        | (KernelEventKind::AcceptanceNeedsChanges, _) => false,
        _ => true,
    }
}
#[doc = " Checks that the candidate did not introduce an accepted run."]
#[allow(
    clippy::semicolon_if_nothing_returned,
    reason = "Verus proof blocks are statements despite Clippy parsing them as expressions"
)]
pub(super) fn no_new_acceptance(before: &KernelAggregate, after: &KernelAggregate) -> bool {
    let mut after_index = 0;
    while after_index < after.runs.len() {
        let after_run = &after.runs[after_index];
        if matches!(after_run.phase, RunPhase::Accepted) {
            let id = after_run.id;
            let Some(before_index) = before.run_index(id) else {
                return false;
            };
            let before_run = &before.runs[before_index];
            match before_run.phase {
                RunPhase::Accepted => {}
                _ => return false,
            }
        }
        after_index += 1;
    }
    true
}
