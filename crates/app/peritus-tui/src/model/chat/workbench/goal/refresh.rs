//! Refresh a stale goal panel before submitting the user's unchanged control command.

use super::{AppModel, Effect, NoticeLevel};

impl AppModel {
    pub(super) fn refresh_stale_goal_control(&mut self) -> Option<Vec<Effect>> {
        let goal = self.chat.workbench.goal.as_ref()?;
        if goal.aggregate_revision() >= self.chat.workbench.receipted_revision {
            return None;
        }
        self.chat.workbench.goal_refresh_command = Some((goal.goal(), self.chat.buffer.clone()));
        Some(self.refresh_goal())
    }

    pub(super) fn complete_goal_control_refresh(&mut self) -> Option<Vec<Effect>> {
        let (expected_goal, draft) = self.chat.workbench.goal_refresh_command.take()?;
        // A read receipt must not consume the command before its mutation is accepted.
        self.chat.workbench.inspection_draft = None;
        if self.chat.buffer != draft {
            return Some(Vec::new());
        }
        if self.chat.workbench.goal.as_ref().is_none_or(|goal| goal.goal() != expected_goal) {
            self.notice(
                NoticeLevel::Warning,
                "The goal changed during refresh. Review it before resubmitting; draft retained.",
            );
            return Some(Vec::new());
        }
        Some(self.slash_command(&draft))
    }
}
