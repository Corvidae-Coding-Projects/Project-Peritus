//! Input, goal, attachment, review, and fork aggregate transitions.

use peritus_types::ActorId;

use super::{ControlError, ControlIntent, ControlOperation, ConversationRecord};

impl ConversationRecord {
    pub(super) fn apply_library(&mut self, intent: &ControlIntent) -> Result<(), ControlError> {
        match intent {
            ControlIntent::RenameConversation { title } => {
                self.title = title.clone();
                Ok(())
            }
            ControlIntent::PinConversation { pinned } => {
                self.pinned = *pinned;
                Ok(())
            }
            ControlIntent::ArchiveConversation { archived } => {
                self.archived = *archived;
                Ok(())
            }
            ControlIntent::ReserveFork { branch, now_unix_millis } => {
                self.reserve_fork(branch, *now_unix_millis)
            }
            ControlIntent::PublishRestoreBranch { restore, branch } => {
                let settled = self
                    .restores
                    .iter()
                    .find(|value| value.id() == *restore)
                    .ok_or(ControlError::NotFound)?;
                if settled.status() != crate::control::RestoreStatus::Applied
                    || settled.branch() != Some(branch)
                {
                    return Err(ControlError::InvalidInput);
                }
                Ok(())
            }
            _ => Err(ControlError::InvalidInput),
        }
    }

    pub(super) fn apply_input(
        &mut self,
        current: &Self,
        operation: &ControlOperation,
        intent: &ControlIntent,
    ) -> Result<(), ControlError> {
        match intent {
            ControlIntent::Queue(intent) => self.apply_queue(operation.actor, intent),
            ControlIntent::SetBrief { field, text } => {
                self.apply_brief(operation.actor, operation.id, *field, text)
            }
            ControlIntent::SetContext { target, preference } => {
                let mut context = current.context.clone();
                context.set(current, *target, *preference)?;
                self.context = context;
                self.inputs = self.inputs.context_changed()?;
                Ok(())
            }
            ControlIntent::ApplyPromptView(view) => self.apply_prompt_view(current, view),
            ControlIntent::UpdateGuidance(_) => {
                self.inputs = self.inputs.context_changed()?;
                Ok(())
            }
            _ => Err(ControlError::InvalidInput),
        }
    }

    pub(super) fn apply_attachment(
        &mut self,
        operation: &ControlOperation,
        intent: &ControlIntent,
    ) -> Result<(), ControlError> {
        match intent {
            ControlIntent::PublishReply(reply) => self.publish_reply(operation, reply),
            ControlIntent::AttachImage { image, text } => {
                self.attach_image(operation.actor, operation.id, image, text)
            }
            ControlIntent::SelectImage { attachment, selected } => {
                self.images.select(&mut self.inputs, *attachment, *selected)
            }
            ControlIntent::AttachFile { .. }
            | ControlIntent::SelectFile { .. }
            | ControlIntent::RefreshFile { .. } => self.apply_file(operation),
            _ => Err(ControlError::InvalidInput),
        }
    }

    pub(super) fn apply_review(
        &mut self,
        current: &Self,
        operation: &ControlOperation,
    ) -> Result<(), ControlError> {
        match operation.intent() {
            ControlIntent::AddReview { anchor, feedback, message } => {
                self.validate_review_anchor(anchor)?;
                (self.reviews, self.inputs) = current.reviews.add(
                    &current.inputs,
                    operation_actor(operation)?,
                    operation.id,
                    anchor.clone(),
                    *feedback,
                    message.clone(),
                )?;
                Ok(())
            }
            ControlIntent::RebindReview { comment, anchor } => {
                self.validate_review_anchor(anchor)?;
                (self.reviews, self.inputs) = current.reviews.rebind(
                    &current.inputs,
                    operation_actor(operation)?,
                    operation.id,
                    *comment,
                    anchor.clone(),
                )?;
                Ok(())
            }
            ControlIntent::DismissReview { comment } => {
                (self.reviews, self.inputs) = current.reviews.dismiss(
                    &current.inputs,
                    operation_actor(operation)?,
                    operation.id,
                    *comment,
                )?;
                Ok(())
            }
            _ => Err(ControlError::InvalidInput),
        }
    }

    pub(super) fn apply_queue(
        &mut self,
        actor: [u8; 16],
        intent: &crate::control::QueueIntent,
    ) -> Result<(), ControlError> {
        self.inputs = self
            .inputs
            .apply(ActorId::new(actor).map_err(|_| ControlError::InvalidInput)?, intent)?;
        self.brief.refresh(&self.inputs)?;
        let generation = self.inputs.generation();
        if let Some(goal) = &mut self.goal {
            goal.requirements_changed(generation, false, goal.updated_unix_millis())?;
        }
        Ok(())
    }

    pub(super) fn apply_brief(
        &mut self,
        actor: [u8; 16],
        operation: crate::control::OperationId,
        field: crate::control::BriefField,
        text: &crate::control::ControlText<8192>,
    ) -> Result<(), ControlError> {
        self.brief.set(
            &mut self.inputs,
            ActorId::new(actor).map_err(|_| ControlError::InvalidInput)?,
            operation,
            field,
            text,
        )?;
        let generation = self.inputs.generation();
        if let Some(goal) = &mut self.goal {
            goal.requirements_changed(
                generation,
                field == crate::control::BriefField::Objective,
                goal.updated_unix_millis(),
            )?;
        }
        Ok(())
    }

    fn attach_image(
        &mut self,
        actor: [u8; 16],
        operation: crate::control::OperationId,
        image: &crate::control::ImageAttachment,
        text: &crate::control::ControlText<8192>,
    ) -> Result<(), ControlError> {
        if image.operation() != operation {
            return Err(ControlError::InvalidInput);
        }
        self.images.attach(
            &mut self.inputs,
            ActorId::new(actor).map_err(|_| ControlError::InvalidInput)?,
            image,
            text,
        )
    }

    fn apply_prompt_view(
        &mut self,
        current: &Self,
        view: &crate::control::PromptView,
    ) -> Result<(), ControlError> {
        let expected =
            current.prompt_view.generation().checked_add(1).ok_or(ControlError::Capacity)?;
        if view.generation() != expected {
            return Err(ControlError::StaleRevision);
        }
        view.validate(&current.replies)?;
        self.prompt_view = view.clone();
        self.inputs = self.inputs.context_changed()?;
        Ok(())
    }

    fn apply_file(&mut self, operation: &ControlOperation) -> Result<(), ControlError> {
        match &operation.intent {
            ControlIntent::AttachFile { file, text } if file.operation() == operation.id => {
                self.files.attach(
                    &mut self.inputs,
                    ActorId::new(operation.actor).map_err(|_| ControlError::InvalidInput)?,
                    file,
                    text,
                )
            }
            ControlIntent::SelectFile { attachment, selected } => {
                self.files.select(&mut self.inputs, *attachment, *selected)
            }
            ControlIntent::RefreshFile { attachment, previous, version }
                if version.operation() == operation.id =>
            {
                self.files.refresh(&mut self.inputs, *attachment, *previous, version)
            }
            _ => Err(ControlError::InvalidInput),
        }
    }

    fn publish_reply(
        &mut self,
        operation: &ControlOperation,
        reply: &crate::control::PublicReplyReference,
    ) -> Result<(), ControlError> {
        if reply.operation() != operation.id
            || self.inputs.invocations().last().map(crate::control::InvocationInputs::invocation)
                != Some(reply.after_invocation())
            || self.replies.iter().any(|prior| prior.after_invocation() == reply.after_invocation())
            || self.replies.len() >= 1024
        {
            return Err(ControlError::InvalidInput);
        }
        self.replies.push(reply.clone());
        self.reviews.observe_reply(&self.inputs, reply.after_invocation(), operation.id);
        Ok(())
    }

    fn validate_review_anchor(
        &self,
        anchor: &crate::control::ReviewAnchor,
    ) -> Result<(), ControlError> {
        let execution = self.execution.as_ref().ok_or(ControlError::InvalidInput)?;
        if anchor.workspace().as_bytes() != &self.workspace
            || anchor.run().as_bytes() != &execution.run
        {
            return Err(ControlError::ScopeMismatch);
        }
        Ok(())
    }
}

fn operation_actor(operation: &ControlOperation) -> Result<ActorId, ControlError> {
    ActorId::new(operation.actor).map_err(|_| ControlError::InvalidInput)
}
