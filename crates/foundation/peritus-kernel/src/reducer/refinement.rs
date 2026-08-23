//! Executable audit of the safety-critical reducer refinement boundary.

use crate::{
    AcceptancePhase, KernelAggregate, KernelEventKind, KernelSubject, RunPhase, SessionPhase,
};
use vstd::prelude::*;

verus! {

/// Checks the concrete session and acceptance edges constrained by the closed proof model.
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
) -> (result: bool)
    ensures result ==> crate::model::legal_concrete_step(before, after, event, subject),
{
    match (event, subject) {
        (KernelEventKind::SessionPaused, KernelSubject::Session(id)) => {
            if before.session.id != id || after.session.id != id { return false; }
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
            if before.session.id != id || after.session.id != id { return false; }
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
            if before.session.id != id || after.session.id != id { return false; }
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
            let Some(before_index) = before.run_index(id) else { return false; };
            let Some(after_index) = after.run_index(id) else { return false; };
            let before_run = &before.runs[before_index];
            let after_run = &after.runs[after_index];
            assert(*before_run == before.runs@[before_index as int]);
            assert(*after_run == after.runs@[after_index as int]);
            proof {
                crate::RunState::equal_model_fields(
                    *before_run,
                    before.runs@[before_index as int],
                );
                crate::RunState::equal_model_fields(*after_run, after.runs@[after_index as int]);
            }
            match before_run.phase { RunPhase::Reviewing => {}, _ => return false }
            match before_run.acceptance {
                AcceptancePhase::Evaluating => {}
                _ => return false,
            }
            match after_run.phase { RunPhase::Accepted => {}, _ => return false }
            match after_run.acceptance {
                AcceptancePhase::Accepted => {}
                _ => return false,
            }
            proof {
                let left = before_index as int;
                let right = after_index as int;
                assert(exists |left: int, right: int| {
                    &&& 0 <= left < before.runs@.len()
                    &&& 0 <= right < after.runs@.len()
                    &&& crate::identity::run_ids_equal(before.runs@[left].id, id)
                    &&& crate::identity::run_ids_equal(after.runs@[right].id, id)
                    &&& before.runs@[left].phase == RunPhase::Reviewing
                    &&& before.runs@[left].acceptance == AcceptancePhase::Evaluating
                    &&& after.runs@[right].phase == RunPhase::Accepted
                    &&& after.runs@[right].acceptance == AcceptancePhase::Accepted
                });
            }
            true
        }
        (KernelEventKind::AcceptanceNeedsChanges, KernelSubject::Acceptance(id)) => {
            let Some(before_index) = before.run_index(id) else { return false; };
            let Some(after_index) = after.run_index(id) else { return false; };
            let before_run = &before.runs[before_index];
            let after_run = &after.runs[after_index];
            assert(*before_run == before.runs@[before_index as int]);
            assert(*after_run == after.runs@[after_index as int]);
            proof {
                crate::RunState::equal_model_fields(
                    *before_run,
                    before.runs@[before_index as int],
                );
                crate::RunState::equal_model_fields(*after_run, after.runs@[after_index as int]);
            }
            match before_run.phase { RunPhase::Reviewing => {}, _ => return false }
            match before_run.acceptance {
                AcceptancePhase::Evaluating => {}
                _ => return false,
            }
            match after_run.phase { RunPhase::Fixing => {}, _ => return false }
            match after_run.acceptance {
                AcceptancePhase::NeedsChanges => {}
                _ => return false,
            }
            proof {
                let left = before_index as int;
                let right = after_index as int;
                assert(exists |left: int, right: int| {
                    &&& 0 <= left < before.runs@.len()
                    &&& 0 <= right < after.runs@.len()
                    &&& crate::identity::run_ids_equal(before.runs@[left].id, id)
                    &&& crate::identity::run_ids_equal(after.runs@[right].id, id)
                    &&& before.runs@[left].phase == RunPhase::Reviewing
                    &&& before.runs@[left].acceptance == AcceptancePhase::Evaluating
                    &&& after.runs@[right].phase == RunPhase::Fixing
                    &&& after.runs@[right].acceptance == AcceptancePhase::NeedsChanges
                });
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

/// Checks that the candidate did not introduce an accepted run.
#[allow(
    clippy::semicolon_if_nothing_returned,
    reason = "Verus proof blocks are statements despite Clippy parsing them as expressions"
)]
pub(super) fn no_new_acceptance(
    before: &KernelAggregate,
    after: &KernelAggregate,
) -> (result: bool)
    ensures result ==> crate::model::no_new_accepted_run(before, after),
{
    let mut after_index = 0;
    while after_index < after.runs.len()
        invariant
            after_index <= after.runs.len(),
            forall |checked: int| #![auto]
                0 <= checked < after_index
                    && after.runs@[checked].phase == RunPhase::Accepted
                ==> exists |before_index: int| {
                    &&& 0 <= before_index < before.runs@.len()
                    &&& crate::identity::run_ids_equal(
                        before.runs@[before_index].id,
                        after.runs@[checked].id,
                    )
                    &&& before.runs@[before_index].phase == RunPhase::Accepted
                },
        decreases after.runs.len() - after_index,
    {
        let after_run = &after.runs[after_index];
        assert(*after_run == after.runs@[after_index as int]);
        proof {
            crate::RunState::equal_model_fields(*after_run, after.runs@[after_index as int]);
        }
        if matches!(after_run.phase, RunPhase::Accepted) {
            let id = after_run.id;
            let Some(before_index) = before.run_index(id) else { return false; };
            let before_run = &before.runs[before_index];
            assert(*before_run == before.runs@[before_index as int]);
            proof {
                crate::RunState::equal_model_fields(
                    *before_run,
                    before.runs@[before_index as int],
                );
            }
            match before_run.phase { RunPhase::Accepted => {}, _ => return false }
            proof {
                let prior = before_index as int;
                let current = after_index as int;
                assert(0 <= prior < before.runs@.len());
                assert(before.runs@[prior].phase == RunPhase::Accepted);
                assert(exists |prior: int| {
                    &&& 0 <= prior < before.runs@.len()
                    &&& crate::identity::run_ids_equal(
                        before.runs@[prior].id,
                        after.runs@[after_index as int].id,
                    )
                    &&& before.runs@[prior].phase == RunPhase::Accepted
                });
            }
        }
        after_index += 1;
    }
    true
}

} // verus!
