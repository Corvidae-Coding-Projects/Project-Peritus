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
mod files;
mod fork;
mod goal;
mod images;
mod init;
mod memory;
mod navigation;
mod permissions;
mod queue;
mod receipts;
mod sessions;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum WorkbenchMode {
    #[default]
    Sessions,
    Queue,
    Brief,
    Compaction,
    Checkpoints,
    Permissions,
    Init,
    Memory,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum WorkbenchMemoryView {
    #[default]
    Current,
    History,
}

impl WorkbenchMemoryView {
    const fn from_include_forgotten(include_forgotten: bool) -> Self {
        if include_forgotten { Self::History } else { Self::Current }
    }

    const fn include_forgotten(self) -> bool {
        matches!(self, Self::History)
    }
}

#[derive(Debug, Default)]
pub struct WorkbenchUi {
    pub(crate) images: images::ImageUi,
    pub(crate) files: files::FileUi,
    pub(crate) open: bool,
    pub(crate) selected: Option<WorkbenchQuery>,
    pub(crate) snapshot: Option<WorkbenchSnapshot>,
    pub(crate) library: Option<peritus_app_protocol::ConversationLibraryPage>,
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
    pub(crate) checkpoint_receipt: Option<peritus_app_protocol::WorkbenchCheckpointReceipt>,
    pub(crate) rewind_request: Option<peritus_app_protocol::WorkbenchRewindRequest>,
    pub(crate) rewind_preview: Option<peritus_app_protocol::WorkbenchRewindPreview>,
    pub(crate) restore_receipt: Option<peritus_app_protocol::WorkbenchRestoreReceipt>,
    pub(crate) permissions: Option<peritus_app_protocol::WorkbenchPermissions>,
    pub(crate) init: Option<peritus_app_protocol::InitProposal>,
    pub(crate) memory: Option<peritus_app_protocol::WorkbenchMemory>,
    memory_view: WorkbenchMemoryView,
    pub(crate) unresolved: Option<(WorkbenchCommand, String)>,
}

impl WorkbenchUi {
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
    fn submit_workbench(
        &mut self,
        intent: WorkbenchIntent,
        workspace: peritus_types::WorkspaceId,
    ) -> Vec<Effect> {
        let Some((query, revision)) = self.workbench_command_binding(&intent, workspace) else {
            return Vec::new();
        };
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
        self.chat.workbench.open = true;
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
            | WorkbenchIntent::UpdateGoalBudget { .. }
            | WorkbenchIntent::ClearGoal { .. } => self.goal_command_binding(workspace),
            WorkbenchIntent::ApplyRewind(preview) => self.rewind_binding(preview, workspace),
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
            self.chat.workbench.mode == WorkbenchMode::Brief
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
