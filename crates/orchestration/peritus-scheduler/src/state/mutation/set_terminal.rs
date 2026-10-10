//! Exact terminal-state mutation.

use crate::{SchedulerPhase, SchedulerState, SchedulerTerminal};

#[doc = " Commits an already evaluated terminal summary and frames every other state field."]
pub fn set_terminal(state: &mut SchedulerState, terminal: SchedulerTerminal) {
    state.phase = SchedulerPhase::Terminal;
    state.terminal = Some(terminal);
}
