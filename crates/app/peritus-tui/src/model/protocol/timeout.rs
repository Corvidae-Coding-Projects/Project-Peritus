//! Read-only timeouts are local failures; uncertain effects still require reconciliation.

use crate::model::{AppModel, NoticeLevel, PendingRequest, PromptPhase};

#[derive(Clone, Copy)]
enum Recovery {
    Read,
    WorkbenchRead,
    Reconnect,
}

impl PendingRequest {
    const fn timeout_recovery(&self) -> Recovery {
        match self {
            Self::Doctor(_)
            | Self::ModelQuery(_)
            | Self::Status
            | Self::ChatBinding { .. }
            | Self::ProductMessageBinding { .. }
            | Self::ChatQuery
            | Self::ChatOpen { .. }
            | Self::ProductQuery
            | Self::ProductExactQuery(_)
            | Self::ProductInteractionQuery
            | Self::TerminalLineInput(_) => Recovery::Read,
            Self::WorkbenchCheckpointInspect(_)
            | Self::WorkbenchRewind(_)
            | Self::WorkbenchCheckpointPage(_)
            | Self::WorkbenchRewindPage(_)
            | Self::WorkbenchMemory(_)
            | Self::WorkbenchInitArtifacts(_)
            | Self::WorkbenchInitArtifactPage(_)
            | Self::WorkbenchInit(_)
            | Self::WorkbenchPermissions(_)
            | Self::WorkbenchCompaction(_)
            | Self::WorkbenchFileImportPreview(_)
            | Self::WorkbenchMessagePreview(_)
            | Self::WorkbenchImagePreview(_)
            | Self::WorkbenchImages(_)
            | Self::WorkbenchFilePreview(_)
            | Self::WorkbenchFiles(_)
            | Self::WorkbenchReview(_)
            | Self::WorkbenchReviewSummary(_)
            | Self::WorkbenchReviewDiff(_)
            | Self::WorkbenchReviewDiffBytes(_)
            | Self::WorkbenchQuery(_)
            | Self::WorkbenchExecution(_)
            | Self::ConversationLibrary(_)
            | Self::ResumeConversationLibrary(_)
            | Self::WorkbenchQueue(_)
            | Self::WorkbenchQueueCommand { .. }
            | Self::WorkbenchContext(_)
            | Self::WorkbenchBrief(_)
            | Self::WorkbenchBriefPage(_)
            | Self::WorkbenchBriefProposal(_)
            | Self::WorkbenchGoal(_)
            | Self::WorkbenchResult(_) => Recovery::WorkbenchRead,
            Self::ArtifactCancel
            | Self::WorkbenchFileUpload { .. }
            | Self::WorkbenchMessageUpload { .. }
            | Self::WorkbenchImageUpload { .. }
            | Self::WorkbenchChatContinue { .. }
            | Self::WorkbenchChatStarted { .. }
            | Self::WorkbenchControl(_)
            | Self::WorkbenchReceipt(_)
            | Self::ModelUpdate { .. }
            | Self::Subscribe
            | Self::Prompt(_)
            | Self::TerminalAttach(_)
            | Self::TerminalInput
            | Self::TerminalResize
            | Self::TerminalDetach(_)
            | Self::TerminalCancel
            | Self::ProductControl => Recovery::Reconnect,
        }
    }
}

impl AppModel {
    pub(in crate::model) fn expire_pending_requests(&mut self) -> bool {
        const REQUEST_TIMEOUT_TICKS: u64 = 120;
        let expired = self
            .pending_started
            .iter()
            .filter_map(|(request, started)| {
                (self.tick_count.saturating_sub(*started) >= REQUEST_TIMEOUT_TICKS)
                    .then_some(*request)
            })
            .collect::<Vec<_>>();
        if expired.is_empty() {
            return false;
        }
        // A workbench read can continue a previously accepted input or reconcile its receipt.
        // Decide before removing anything so request iteration order cannot change recovery.
        let workbench_pending = self.workbench_chat_pending();
        let mut reconnect = false;
        let mut unconfirmed_terminal_input = false;
        for request in expired {
            self.pending_started.remove(&request);
            let pending = self.pending.remove(&request);
            if let Some(editor) = self.pending_editor_drafts.remove(&request) {
                let ambiguous =
                    pending.as_ref().is_none_or(PendingRequest::editor_outcome_is_ambiguous);
                self.restore_editor(editor, ambiguous);
            }
            let Some(pending) = pending else { continue };
            match pending.timeout_recovery() {
                Recovery::Read => {
                    if matches!(pending, PendingRequest::Doctor(_))
                        && let Some(panel) = &mut self.chat.doctor
                    {
                        panel.error = Some(
                            "Diagnostic request timed out; r retries, Esc returns. Draft retained."
                                .to_owned(),
                        );
                    }
                }
                Recovery::WorkbenchRead if !workbench_pending => {
                    self.workbench_inspection_timeout(&pending);
                }
                Recovery::Reconnect | Recovery::WorkbenchRead => reconnect = true,
            }
            match pending {
                PendingRequest::TerminalLineInput(binding) => {
                    if let Some(terminal) = &mut self.terminal
                        && terminal.binding() == binding
                    {
                        terminal.settle_line_input(false);
                        unconfirmed_terminal_input = true;
                    }
                }
                PendingRequest::Prompt(prompt) => {
                    self.set_prompt_phase(prompt, PromptPhase::Failed);
                }
                _ => {}
            }
        }
        self.notice(NoticeLevel::Error, if reconnect {
            "daemon request timed out; reconnecting to reconcile its outcome before another action"
        } else if unconfirmed_terminal_input {
            "Terminal input acknowledgement timed out. Delivery is unknown; draft retained. Inspect the child before resending. Chat remains connected."
        } else {
            "Inspection timed out. Draft and connection retained; refresh the panel or retry the command. Ctrl-R reconnects if needed."
        });
        reconnect
    }
}
