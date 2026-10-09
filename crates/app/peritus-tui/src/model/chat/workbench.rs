//! Durable control intent tracking; reconnect resolves the original operation, never invents one.

use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::{
    AppRequestPayload, ControlOperationId, ConversationId, WorkbenchCommand, WorkbenchIntent,
    WorkbenchQuery, WorkbenchSnapshot,
};

mod brief;
mod checkpoints;
mod compaction;
mod context;
mod conversation;
mod files;
mod fork;
mod goal;
mod images;
mod init;
mod library;
mod memory;
mod navigation;
mod permissions;
mod queue;
mod receipts;
mod refresh;
mod sessions;
mod state;
use state::{WorkbenchMemoryView, WorkbenchMode};

#[derive(Debug, Default)]
pub struct WorkbenchUi {
    pub(crate) images: images::ImageUi,
    pub(crate) files: files::FileUi,
    pub(crate) open: bool,
    pub(crate) selected: Option<WorkbenchQuery>,
    pub(crate) snapshot: Option<WorkbenchSnapshot>,
    pub(crate) library: Option<peritus_app_protocol::ConversationLibraryPage>,
    library_query: Option<peritus_app_protocol::ConversationLibraryQuery>,
    pub(crate) library_selected: usize,
    pub(crate) scroll: usize,
    pub(crate) message: String,
    mode: WorkbenchMode,
    pub(crate) queue: Option<peritus_app_protocol::WorkbenchQueuePage>,
    pub(crate) queue_detail: Option<peritus_app_protocol::WorkbenchInputSelection>,
    pub(crate) context_mode: Option<peritus_app_protocol::WorkbenchContextView>,
    pub(crate) context_page: Option<peritus_app_protocol::WorkbenchContextPage>,
    pub(crate) compaction_request: Option<peritus_app_protocol::WorkbenchCompactionRequest>,
    pub(crate) compaction_preview: Option<peritus_app_protocol::WorkbenchCompactionPreview>,
    pub(crate) brief: Option<peritus_app_protocol::WorkbenchBrief>,
    pub(crate) goal_mode: bool,
    pub(crate) goal: Option<peritus_app_protocol::WorkbenchGoalSnapshot>,
    pub(crate) goal_draft: Option<goal::GoalDraft>,
    pub(crate) goal_clear_pending: bool,
    pub(crate) goal_confirm_pending: Option<peritus_app_protocol::WorkbenchInputText>,
    receipted_revision: u64,
    goal_refresh_command: Option<(ControlOperationId, String)>,
    pub(in crate::model::chat) snapshot_refresh_command: Option<(WorkbenchQuery, String, String)>,
    pub(crate) checkpoint_receipt: Option<peritus_app_protocol::WorkbenchCheckpointReceipt>,
    pub(crate) checkpoint_page: Option<peritus_app_protocol::WorkbenchCheckpointCoveragePage>,
    pub(crate) checkpoint_page_request:
        Option<peritus_app_protocol::WorkbenchCheckpointPageRequest>,
    pub(crate) checkpoint_page_history: Vec<Option<peritus_app_protocol::WorkbenchCoverageCursor>>,
    pub(crate) rewind_request: Option<peritus_app_protocol::WorkbenchRewindRequest>,
    pub(crate) rewind_preview: Option<peritus_app_protocol::WorkbenchRewindPreview>,
    pub(crate) rewind_page: Option<peritus_app_protocol::WorkbenchRewindCoveragePage>,
    pub(crate) rewind_page_request: Option<peritus_app_protocol::WorkbenchRewindPageRequest>,
    pub(crate) rewind_page_history: Vec<Option<peritus_app_protocol::WorkbenchCoverageCursor>>,
    pub(crate) restore_receipt: Option<peritus_app_protocol::WorkbenchRestoreReceipt>,
    pub(crate) restore_summary: Option<peritus_app_protocol::WorkbenchRestoreSummary>,
    pub(crate) permissions: Option<peritus_app_protocol::WorkbenchPermissions>,
    pub(crate) init: Option<peritus_app_protocol::InitProposal>,
    pub(crate) memory: Option<peritus_app_protocol::WorkbenchMemory>,
    memory_view: WorkbenchMemoryView,
    pub(crate) unresolved: Option<(WorkbenchCommand, String)>,
    rejected_control: Option<peritus_app_protocol::AppErrorCode>,
    pub(crate) inspection_draft: Option<String>,
    submission: Option<conversation::Submission>,
}

impl WorkbenchUi {
    pub(crate) const fn library_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Library)
    }

    pub(crate) const fn queue_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Queue)
    }

    pub(crate) const fn brief_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Brief)
    }

    pub(crate) const fn compaction_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Compaction)
    }

    pub(crate) const fn checkpoints_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Checkpoints)
    }

    pub(crate) const fn permissions_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Permissions)
    }

    pub(crate) const fn init_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Init)
    }

    pub(crate) const fn memory_open(&self) -> bool {
        matches!(self.mode, WorkbenchMode::Memory)
    }

    const fn include_forgotten_memory(&self) -> bool {
        self.memory_view.include_forgotten()
    }

    const fn set_include_forgotten_memory(&mut self, include_forgotten: bool) {
        self.memory_view = WorkbenchMemoryView::from_include_forgotten(include_forgotten);
    }
}

impl AppModel {
    pub(in crate::model) fn select_workbench_conversation(
        &mut self,
        query: Option<WorkbenchQuery>,
    ) {
        self.cancel_latest_resume();
        if self.chat.workbench.selected == query {
            return;
        }
        self.abandon_chat_observations();
        self.abandon_workbench_inspection();
        let library = self.chat.workbench.library.take();
        let open = self.chat.workbench.open;
        self.chat.workbench = WorkbenchUi::default();
        self.chat.workbench.open = open;
        self.chat.workbench.library = library;
        self.chat.workbench.selected = query;
        self.chat.run_id = None;
        self.chat.snapshot = None;
    }

    pub(in crate::model) fn complete_workbench_inspection(&mut self) {
        if let Some(draft) = self.chat.workbench.inspection_draft.take()
            && self.chat.buffer == draft
        {
            self.clear_chat_command();
        }
    }

    fn submit_workbench(
        &mut self,
        intent: WorkbenchIntent,
        workspace: peritus_types::WorkspaceId,
    ) -> Vec<Effect> {
        let Some((query, revision)) = self.workbench_command_binding(&intent, workspace) else {
            return Vec::new();
        };
        self.submit_bound_workbench(intent, query, revision)
    }

    fn submit_bound_workbench(
        &mut self,
        intent: WorkbenchIntent,
        query: WorkbenchQuery,
        revision: u64,
    ) -> Vec<Effect> {
        let effects = self.send_bound_workbench_command(intent, query, revision);
        if !effects.is_empty() {
            self.chat.workbench.open = true;
        }
        effects
    }

    fn send_bound_workbench_command(
        &mut self,
        intent: WorkbenchIntent,
        query: WorkbenchQuery,
        revision: u64,
    ) -> Vec<Effect> {
        let Ok(operation) = ControlOperationId::new(self.ids.bytes(b"workbench-operation")) else {
            return Vec::new();
        };
        let command = WorkbenchCommand::new(operation, query, revision, intent);
        let Some(effect) = self.request(
            AppRequestPayload::WorkbenchCommand(command.clone()),
            PendingRequest::WorkbenchControl(command.clone()),
        ) else {
            return Vec::new();
        };
        self.chat.workbench.unresolved = Some((command, self.chat.buffer.clone()));
        self.chat.workbench.rejected_control = None;
        "Awaiting durable receipt; not yet accepted.".clone_into(&mut self.chat.workbench.message);
        vec![effect]
    }

    fn workbench_command_binding(
        &mut self,
        intent: &WorkbenchIntent,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        match intent {
            WorkbenchIntent::CreateConversation(_) => self.new_conversation_binding(workspace),
            WorkbenchIntent::AttachImage { preview, .. } => {
                self.image_preview_binding(preview, workspace)
            }
            WorkbenchIntent::SelectImage { .. } => self.image_page_binding(workspace),
            WorkbenchIntent::SetBrief { .. } | WorkbenchIntent::AcceptBriefProposal { .. } => {
                self.brief_command_binding(workspace)
            }
            WorkbenchIntent::SetContext { .. } => self.context_command_binding(workspace),
            WorkbenchIntent::ApplyCompaction(preview) => self.compaction_command_binding(preview),
            WorkbenchIntent::StartGoal { .. } => self.goal_start_binding(workspace),
            WorkbenchIntent::PauseGoal { .. }
            | WorkbenchIntent::ResumeGoal { .. }
            | WorkbenchIntent::ClearGoal { .. } => self.goal_command_binding(workspace),
            WorkbenchIntent::ApplyRewind(preview) => self.rewind_binding(preview, workspace),
            WorkbenchIntent::ConfirmRewind(confirmation) => {
                self.rewind_confirmation_binding(*confirmation, workspace)
            }
            WorkbenchIntent::SetPermissions(_) => self.permission_command_binding(workspace),
            WorkbenchIntent::ApplyInitDiff(proposal) => {
                self.init_command_binding(proposal, workspace)
            }
            WorkbenchIntent::SaveGuidance(_)
            | WorkbenchIntent::ReviseGuidance(_)
            | WorkbenchIntent::PinGuidance(_)
            | WorkbenchIntent::ScopeGuidance(_)
            | WorkbenchIntent::ForgetGuidance(_) => self.memory_command_binding(workspace),
            _ => self.inspected_command_binding(workspace),
        }
    }

    fn new_conversation_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let id = ConversationId::new(self.ids.bytes(b"workbench-conversation")).ok()?;
        Some((WorkbenchQuery::new(id, workspace), 0))
    }

    fn image_preview_binding(
        &mut self,
        preview: &peritus_app_protocol::WorkbenchImagePreview,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        if preview.request().query().workspace() != workspace
            || self.chat.workbench.selected != Some(preview.request().query())
        {
            self.notice(
                NoticeLevel::Warning,
                "Image preview belongs to another conversation; draft retained.",
            );
            return None;
        }
        Some((preview.request().query(), preview.request().revision()))
    }

    fn brief_command_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let Some(brief) = self.chat.workbench.brief.as_ref().filter(|brief| {
            (self.chat.workbench.mode == WorkbenchMode::Brief || self.chat.workbench.goal_mode)
                && brief.query().workspace() == workspace
                && self.chat.workbench.selected == Some(brief.query())
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /brief before editing a field; draft retained.",
            );
            return None;
        };
        Some((brief.query(), brief.revision()))
    }

    fn context_command_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let Some(page) = self.chat.workbench.context_page.as_ref().filter(|page| {
            self.chat.workbench.context_mode
                == Some(peritus_app_protocol::WorkbenchContextView::Next)
                && page.query().view() == peritus_app_protocol::WorkbenchContextView::Next
                && page.query().query().workspace() == workspace
                && self.chat.workbench.selected == Some(page.query().query())
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /context next before changing a source preference; draft retained.",
            );
            return None;
        };
        Some((page.query().query(), page.query().revision()))
    }

    fn compaction_command_binding(
        &mut self,
        preview: &peritus_app_protocol::WorkbenchCompactionPreview,
    ) -> Option<(WorkbenchQuery, u64)> {
        if self.chat.workbench.mode != WorkbenchMode::Compaction
            || self.chat.workbench.compaction_preview.as_ref() != Some(preview)
            || self.chat.workbench.selected != Some(preview.request().query())
        {
            self.notice(
                NoticeLevel::Warning,
                "Preview /compact before applying a prompt view; draft retained.",
            );
            return None;
        }
        Some((preview.request().query(), preview.request().revision()))
    }

    fn goal_start_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let Some(brief) = self.chat.workbench.brief.as_ref().filter(|brief| {
            self.chat.workbench.goal_mode
                && brief.query().workspace() == workspace
                && self.chat.workbench.selected == Some(brief.query())
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect the confirmed brief before starting this goal; draft retained.",
            );
            return None;
        };
        Some((brief.query(), brief.revision()))
    }

    fn goal_command_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let Some(goal) = self.chat.workbench.goal.as_ref().filter(|goal| {
            self.chat.workbench.goal_mode
                && goal.query().workspace() == workspace
                && self.chat.workbench.selected == Some(goal.query())
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect the current goal before changing it; draft retained.",
            );
            return None;
        };
        Some((goal.query(), goal.aggregate_revision()))
    }

    fn inspected_command_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        if let Some(page) = self.chat.workbench.queue.as_ref().filter(|page| {
            self.chat.workbench.mode == WorkbenchMode::Queue
                && page.query().query().workspace() == workspace
                && self.chat.workbench.selected == Some(page.query().query())
        }) {
            return Some((page.query().query(), page.query().revision()));
        }
        if let Some(snapshot) = self
            .chat
            .workbench
            .snapshot
            .as_ref()
            .filter(|snapshot| snapshot.query().workspace() == workspace)
        {
            return Some((snapshot.query(), snapshot.revision()));
        }
        self.notice(
            NoticeLevel::Warning,
            "Open and inspect a conversation before changing its metadata; draft retained.",
        );
        None
    }
}
