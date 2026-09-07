//! Bounded single-owner reuse of a verified scheduler replay.

use peritus_journal::{CommittedBatch, ReplayObservation, SqliteJournal};
use peritus_types::RunId;

use crate::durability::{binding_error, commit_transition, journal_error};
use crate::{SchedulerCommand, SchedulerError, SchedulerState, SchedulerTransition};

struct CachedReplay {
    run_id: RunId,
    state: Option<SchedulerState>,
    observation: ReplayObservation,
}

/// One verified scheduler aggregate retained by a serialized journal owner.
///
/// Memory is bounded to one aggregate. Switching runs, reopening the journal, another append
/// attempt or an external commit requires full checked replay. A successor is installed here
/// only after its exact guarded durable receipt. Historical-event readers remain independent.
#[derive(Default)]
pub struct SchedulerSession {
    cached: Option<CachedReplay>,
}

impl SchedulerSession {
    /// Borrows current state, replaying and checking the complete checkpoint when invalidated.
    ///
    /// # Errors
    /// Returns the existing journal, codec, replay, or checkpoint-integrity failure.
    pub fn state(
        &mut self,
        journal: &SqliteJournal,
        run_id: RunId,
    ) -> Result<Option<&SchedulerState>, SchedulerError> {
        let cached = self.cached.take();
        let reusable = if let Some(cached) = &cached {
            cached.run_id == run_id
                && journal
                    .replay_observation_is_current(&cached.observation)
                    .map_err(journal_error)?
        } else {
            false
        };
        self.cached = if reusable {
            cached
        } else {
            let observation = journal.observe_replay().map_err(journal_error)?;
            let state = crate::load_scheduler_replay(journal, run_id)?.rebuild()?;
            Some(CachedReplay { run_id, state, observation })
        };
        Ok(self.cached.as_ref().and_then(|cached| cached.state.as_ref()))
    }

    /// Commits a pure transition from the previously observed state and retains its successor.
    ///
    /// # Errors
    /// Rejects transitions not fenced to this session's replay and invalidated observations.
    /// Any failure discards the cache; indeterminate commands still require exact resolution.
    pub fn commit(
        &mut self,
        journal: &mut SqliteJournal,
        command: &SchedulerCommand,
        transition: SchedulerTransition,
    ) -> Result<CommittedBatch, SchedulerError> {
        let cached =
            self.cached.take().ok_or_else(|| binding_error("scheduler session has no replay"))?;
        let prior_matches = cached.state.as_ref().map_or_else(
            || command.expected_sequence() == 0 && command.expected_previous_event().is_none(),
            |state| {
                command.expected_sequence() == state.sequence().get()
                    && command.expected_previous_event() == Some(state.last_event_id())
                    && command.prior_state_digest() == state.state_digest()
            },
        );
        if cached.run_id != command.run_id() || !prior_matches {
            return Err(binding_error(
                "scheduler transition does not extend this session's replay",
            ));
        }
        let (batch, observation) =
            commit_transition(journal, command, &transition, Some(&cached.observation))?;
        if let Some(observation) = observation {
            self.cached = Some(CachedReplay {
                run_id: command.run_id(),
                state: Some(transition.into_state()),
                observation,
            });
        }
        Ok(batch)
    }
}
