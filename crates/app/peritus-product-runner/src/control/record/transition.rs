//! Pure expected-revision successor planning and receipt binding.

use super::{
    CONTROL_SCHEMA, ControlError, ControlIntent, ControlOperation, ControlReceipt,
    ConversationRecord, sha256,
};

mod checkpoint;
mod goal;
mod mutations;

impl ConversationRecord {
    /// Plans one metadata transition without filesystem, provider, or journal effects.
    ///
    /// # Errors
    /// Rejects stale revisions, wrong scope/ownership, duplicate creation, and unsupported data.
    pub fn apply(
        current: Option<&Self>,
        operation: &ControlOperation,
    ) -> Result<(Self, ControlReceipt), ControlError> {
        operation.validate()?;
        if let Some(current) = current {
            current.validate()?;
            if current.id != operation.conversation
                || current.owner != operation.actor
                || current.workspace != operation.workspace
            {
                return Err(ControlError::ScopeMismatch);
            }
        }
        let revision = current.map_or(0, Self::revision);
        if revision != operation.expected_revision {
            return Err(ControlError::StaleRevision);
        }
        let next_revision = revision.checked_add(1).ok_or(ControlError::Capacity)?;
        let mut next = if let Some(current) = current {
            let mut next = current.clone();
            next.apply_existing(current, operation)?;
            next
        } else {
            Self::create(operation)?
        };
        next.revision = next_revision;
        next.validate()?;
        let receipt = Self::receipt(operation, next_revision)?;
        Ok((next, receipt))
    }

    fn create(operation: &ControlOperation) -> Result<Self, ControlError> {
        let (title, branch) = match &operation.intent {
            ControlIntent::CreateConversation { title } => (title.clone(), None),
            ControlIntent::CreateFork { branch } => {
                (crate::control::ControlText::new(branch.title().to_owned())?, Some(branch))
            }
            _ => return Err(ControlError::NotFound),
        };
        let mut record = Self {
            schema: CONTROL_SCHEMA,
            minimum_reader: CONTROL_SCHEMA,
            id: operation.conversation,
            owner: operation.actor,
            workspace: operation.workspace,
            revision: 0,
            title,
            pinned: false,
            archived: false,
            inputs: crate::control::InputLedger::default(),
            brief: crate::control::TaskBrief::default(),
            context: crate::control::ContextSelections::default(),
            prompt_view: crate::control::PromptView::default(),
            images: crate::control::ImageAttachments::default(),
            files: crate::control::FileAttachments::default(),
            reviews: crate::control::ReviewLedger::default(),
            execution: None,
            goal: None,
            replies: Vec::new(),
            checkpoints: Vec::new(),
            restores: Vec::new(),
        };
        if let Some(seed) = branch.and_then(crate::control::ConversationBranch::seed) {
            record.install_seed(seed);
        } else if let Some(branch) = branch
            && let Some(objective) = branch.objective()
        {
            let objective = crate::control::ControlText::new(objective.to_owned())?;
            record.apply_brief(
                operation.actor,
                operation.id,
                crate::control::BriefField::Objective,
                &objective,
            )?;
        }
        Ok(record)
    }

    fn receipt(
        operation: &ControlOperation,
        accepted_revision: u64,
    ) -> Result<ControlReceipt, ControlError> {
        Ok(ControlReceipt {
            schema: CONTROL_SCHEMA,
            operation: operation.id,
            conversation: operation.conversation,
            accepted_revision,
            payload_digest: sha256(&operation.canonical_bytes()?).into_bytes(),
        })
    }

    fn apply_existing(
        &mut self,
        current: &Self,
        operation: &ControlOperation,
    ) -> Result<(), ControlError> {
        match operation.intent() {
            ControlIntent::CreateConversation { .. } | ControlIntent::CreateFork { .. } => {
                Err(ControlError::IdempotencyConflict)
            }
            intent @ (ControlIntent::RenameConversation { .. }
            | ControlIntent::PinConversation { .. }
            | ControlIntent::ArchiveConversation { .. }
            | ControlIntent::ReserveFork { .. }
            | ControlIntent::PublishRestoreBranch { .. }) => self.apply_library(intent),
            intent @ (ControlIntent::Queue(_)
            | ControlIntent::SetBrief { .. }
            | ControlIntent::SetContext { .. }
            | ControlIntent::ApplyPromptView(_)
            | ControlIntent::UpdateGuidance(_)) => self.apply_input(current, operation, intent),
            intent @ (ControlIntent::StartExecution { .. }
            | ControlIntent::StartGoal { .. }
            | ControlIntent::PauseGoal { .. }
            | ControlIntent::ResumeGoal { .. }
            | ControlIntent::UpdateGoalBudget { .. }
            | ControlIntent::ClearGoal { .. }
            | ControlIntent::ReserveGoalRequest { .. }
            | ControlIntent::CompleteGoalRequest { .. }
            | ControlIntent::ReserveGoalTool { .. }
            | ControlIntent::CompleteGoalTool { .. }
            | ControlIntent::ObserveGoalProgress { .. }
            | ControlIntent::SettleGoal { .. }
            | ControlIntent::ObserveGraphicalGoalEvidence { .. }) => {
                self.apply_execution(operation, intent)
            }
            intent @ (ControlIntent::PublishReply(_)
            | ControlIntent::AttachImage { .. }
            | ControlIntent::SelectImage { .. }
            | ControlIntent::AttachFile { .. }
            | ControlIntent::SelectFile { .. }
            | ControlIntent::RefreshFile { .. }) => self.apply_attachment(operation, intent),
            ControlIntent::SetPermissions { .. } | ControlIntent::RecordInitialization { .. } => {
                // The daemon commits the workspace-scoped sidecar in the same C0 transaction.
                Ok(())
            }
            ControlIntent::CreateCheckpoint(_)
            | ControlIntent::SealCheckpoint { .. }
            | ControlIntent::PrepareRestore { .. }
            | ControlIntent::SettleRestore { .. } => self.apply_checkpoint(operation),
            ControlIntent::AddReview { .. }
            | ControlIntent::RebindReview { .. }
            | ControlIntent::DismissReview { .. } => self.apply_review(current, operation),
        }
    }
}
