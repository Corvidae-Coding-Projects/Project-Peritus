//! Exact authenticated public-intent to durable domain-operation mapping.

use super::{ControlStore, Error, brief, files, goal, guidance, images, inputs, review};
use peritus_app_protocol::{WorkbenchCommand, WorkbenchIntent};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ControlText, ConversationId, OperationId,
};
use peritus_types::ActorId;

pub(super) fn equivalent_user_intent(left: &ControlIntent, right: &ControlIntent) -> bool {
    match (left, right) {
        (
            ControlIntent::StartGoal {
                run: left_run,
                settings_digest: left_settings,
                objective: left_objective,
                criteria: left_criteria,
                budget: left_budget,
                ..
            },
            ControlIntent::StartGoal {
                run: right_run,
                settings_digest: right_settings,
                objective: right_objective,
                criteria: right_criteria,
                budget: right_budget,
                ..
            },
        ) => {
            (left_run, left_settings, left_objective, left_criteria, left_budget)
                == (right_run, right_settings, right_objective, right_criteria, right_budget)
        }
        (
            ControlIntent::PauseGoal { goal: a, mode: b, .. },
            ControlIntent::PauseGoal { goal: c, mode: d, .. },
        ) => (a, b) == (c, d),
        (ControlIntent::ResumeGoal { goal: a, .. }, ControlIntent::ResumeGoal { goal: b, .. })
        | (ControlIntent::ClearGoal { goal: a, .. }, ControlIntent::ClearGoal { goal: b, .. }) => {
            a == b
        }
        (
            ControlIntent::UpdateGoalBudget { goal: a, budget: b, .. },
            ControlIntent::UpdateGoalBudget { goal: c, budget: d, .. },
        ) => (a, b) == (c, d),
        _ => left == right,
    }
}

pub(super) fn domain_operation(
    actor: ActorId,
    command: &WorkbenchCommand,
) -> Result<ControlOperation, Error> {
    domain_operation_resolved(actor, command, None, None, None)
}

pub(super) fn domain_operation_with_store(
    store: &ControlStore,
    actor: ActorId,
    command: &WorkbenchCommand,
) -> Result<ControlOperation, Error> {
    let proposal =
        if let WorkbenchIntent::AcceptBriefProposal { proposal, digest, .. } = command.intent() {
            let id = ConversationId::new(command.query().conversation().into_bytes())?;
            let record = store.load(id)?.ok_or(ControlError::NotFound)?;
            if record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != command.query().workspace().as_bytes()
            {
                return Err(ControlError::ScopeMismatch.into());
            }
            let reference = record
                .replies()
                .iter()
                .find(|reply| reply.operation().as_bytes() == proposal.as_bytes())
                .ok_or(ControlError::NotFound)?;
            if reference.digest() != *digest {
                return Err(ControlError::StaleRevision.into());
            }
            Some(ControlText::new(store.reply_text(reference)?)?)
        } else {
            None
        };
    let context = if let WorkbenchIntent::SetContext { source, .. } = command.intent() {
        Some(domain_context_target(store, actor, command, *source)?)
    } else {
        None
    };
    let prompt_view = if let WorkbenchIntent::ApplyCompaction(preview) = command.intent() {
        let (view, current) = store.compaction_preview(actor, preview.request(), true)?;
        if current != *preview {
            return Err(ControlError::StaleRevision.into());
        }
        Some(view.ok_or(ControlError::InvalidInput)?)
    } else {
        None
    };
    domain_operation_resolved(actor, command, proposal, context, prompt_view)
}

fn domain_operation_resolved(
    actor: ActorId,
    command: &WorkbenchCommand,
    proposal: Option<ControlText<8192>>,
    context: Option<peritus_product_runner::control::ContextTarget>,
    prompt_view: Option<peritus_product_runner::control::PromptView>,
) -> Result<ControlOperation, Error> {
    let intent = match command.intent() {
        WorkbenchIntent::ForkConversation(_) => {
            return Err(ControlError::UnsupportedSchema.into());
        }
        WorkbenchIntent::CreateConversation(title) => ControlIntent::CreateConversation {
            title: ControlText::new(title.as_str().to_owned())?,
        },
        WorkbenchIntent::RenameConversation(title) => ControlIntent::RenameConversation {
            title: ControlText::new(title.as_str().to_owned())?,
        },
        WorkbenchIntent::PinConversation(pinned) => {
            ControlIntent::PinConversation { pinned: *pinned }
        }
        WorkbenchIntent::ArchiveConversation(archived) => {
            ControlIntent::ArchiveConversation { archived: *archived }
        }
        WorkbenchIntent::Queue(queue) => ControlIntent::Queue(inputs::domain_intent(queue)?),
        WorkbenchIntent::SetBrief { field, text } => ControlIntent::SetBrief {
            field: brief::domain_field(*field),
            text: ControlText::new(text.as_str().to_owned())?,
        },
        WorkbenchIntent::AcceptBriefProposal { field, .. } => ControlIntent::SetBrief {
            field: brief::domain_field(*field),
            text: proposal.ok_or(ControlError::InvalidInput)?,
        },
        WorkbenchIntent::SetContext { preference, .. } => ControlIntent::SetContext {
            target: context.ok_or(ControlError::InvalidInput)?,
            preference: preference.map(|value| match value {
                peritus_app_protocol::WorkbenchContextPreference::Pinned => {
                    peritus_product_runner::control::ContextPreference::Pinned
                }
                peritus_app_protocol::WorkbenchContextPreference::Excluded => {
                    peritus_product_runner::control::ContextPreference::Excluded
                }
            }),
        },
        WorkbenchIntent::ApplyCompaction(_) => {
            ControlIntent::ApplyPromptView(prompt_view.ok_or(ControlError::InvalidInput)?)
        }
        WorkbenchIntent::AttachImage { preview, text } => ControlIntent::AttachImage {
            image: images::domain_image(command, preview)?,
            text: ControlText::new(text.as_str().to_owned())?,
        },
        WorkbenchIntent::SelectImage { attachment, selected } => ControlIntent::SelectImage {
            attachment: OperationId::new(attachment.into_bytes())?,
            selected: *selected,
        },
        WorkbenchIntent::AttachFile { preview, text } => ControlIntent::AttachFile {
            file: files::domain_file(command, preview)?,
            text: ControlText::new(text.as_str().to_owned())?,
        },
        WorkbenchIntent::AttachFileImport { preview, text } => ControlIntent::AttachFile {
            file: files::domain_import(command, preview)?,
            text: ControlText::new(text.as_str().to_owned())?,
        },
        WorkbenchIntent::SelectFile { attachment, selected } => ControlIntent::SelectFile {
            attachment: OperationId::new(attachment.into_bytes())?,
            selected: *selected,
        },
        WorkbenchIntent::AddReview { anchor, feedback, message } => ControlIntent::AddReview {
            anchor: review::domain_anchor(anchor)?,
            feedback: review::domain_feedback(*feedback),
            message: ControlText::new(message.as_str().to_owned())?,
        },
        WorkbenchIntent::RebindReview { comment, anchor } => ControlIntent::RebindReview {
            comment: OperationId::new(comment.into_bytes())?,
            anchor: review::domain_anchor(anchor)?,
        },
        WorkbenchIntent::DismissReview { comment } => {
            ControlIntent::DismissReview { comment: OperationId::new(comment.into_bytes())? }
        }
        WorkbenchIntent::StartExecution(settings) => ControlIntent::StartExecution {
            run: settings.run().into_bytes(),
            settings_digest: settings
                .fingerprint()
                .map_err(|_| ControlError::InvalidInput)?
                .into_bytes(),
        },
        WorkbenchIntent::StartGoal { definition, settings } => ControlIntent::StartGoal {
            run: settings.run().into_bytes(),
            settings_digest: settings
                .fingerprint()
                .map_err(|_| ControlError::InvalidInput)?
                .into_bytes(),
            objective: ControlText::new(definition.objective().as_str().to_owned())?,
            criteria: definition
                .criteria()
                .iter()
                .map(goal::domain_criterion)
                .collect::<Result<Vec<_>, _>>()?,
            budget: goal::domain_budget(definition.budget())?,
            now_unix_millis: goal::now_millis(),
        },
        WorkbenchIntent::PauseGoal { goal, mode } => ControlIntent::PauseGoal {
            goal: OperationId::new(goal.into_bytes())?,
            mode: goal::domain_pause(*mode),
            now_unix_millis: goal::now_millis(),
        },
        WorkbenchIntent::ResumeGoal { goal } => ControlIntent::ResumeGoal {
            goal: OperationId::new(goal.into_bytes())?,
            now_unix_millis: goal::now_millis(),
        },
        WorkbenchIntent::UpdateGoalBudget { goal, budget } => ControlIntent::UpdateGoalBudget {
            goal: OperationId::new(goal.into_bytes())?,
            budget: goal::domain_budget(*budget)?,
            now_unix_millis: goal::now_millis(),
        },
        WorkbenchIntent::ClearGoal { goal } => ControlIntent::ClearGoal {
            goal: OperationId::new(goal.into_bytes())?,
            now_unix_millis: goal::now_millis(),
        },
        WorkbenchIntent::StartPreview(_)
        | WorkbenchIntent::InteractPreview { .. }
        | WorkbenchIntent::CapturePreview(_)
        | WorkbenchIntent::StopPreview { .. }
        | WorkbenchIntent::CheckPreviewBehavior { .. }
        | WorkbenchIntent::AddArtifactFeedback { .. } => {
            return Err(ControlError::UnsupportedSchema.into());
        }
        WorkbenchIntent::CreateCheckpoint(_) | WorkbenchIntent::ApplyRewind(_) => {
            return Err(ControlError::InvalidInput.into());
        }
        WorkbenchIntent::SetPermissions(change) => ControlIntent::SetPermissions {
            expected_policy_revision: change.expected_authority_revision(),
            capability: super::super::permissions::domain_capability(change.capability()),
            allowed: change.allowed(),
        },
        WorkbenchIntent::SaveGuidance(_)
        | WorkbenchIntent::ReviseGuidance(_)
        | WorkbenchIntent::PinGuidance(_)
        | WorkbenchIntent::ScopeGuidance(_)
        | WorkbenchIntent::ForgetGuidance(_) => guidance::domain_intent(command.intent())?,
        WorkbenchIntent::ApplyInitDiff(_) => return Err(ControlError::InvalidInput.into()),
    };
    Ok(ControlOperation::new(
        OperationId::new(command.operation().into_bytes())?,
        ConversationId::new(command.query().conversation().into_bytes())?,
        actor,
        command.query().workspace(),
        command.expected_revision(),
        intent,
    ))
}

fn domain_context_target(
    store: &ControlStore,
    actor: ActorId,
    command: &WorkbenchCommand,
    source: peritus_app_protocol::WorkbenchContextSource,
) -> Result<peritus_product_runner::control::ContextTarget, Error> {
    use peritus_app_protocol::WorkbenchContextSource as Source;
    use peritus_product_runner::control::{ContextTarget, InputId, InputSelection, InvocationId};

    let id = ConversationId::new(command.query().conversation().into_bytes())?;
    let record = store.load(id)?.ok_or(ControlError::NotFound)?;
    if record.owner_bytes() != actor.as_bytes()
        || record.workspace_bytes() != command.query().workspace().as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    match source {
        Source::Input(selected) => {
            let selected = InputSelection::new(
                InputId::new(selected.id().into_bytes())?,
                selected.revision(),
            )?;
            if record
                .inputs()
                .latest(selected.id())
                .is_none_or(|input| input.selection() != selected)
            {
                return Err(ControlError::StaleRevision.into());
            }
            Ok(ContextTarget::Input(selected))
        }
        Source::PublicReply(invocation) => {
            let invocation = InvocationId::new(invocation.into_bytes())?;
            if !record.replies().iter().any(|reply| reply.after_invocation() == invocation) {
                return Err(ControlError::NotFound.into());
            }
            Ok(ContextTarget::PublicReply(invocation))
        }
        Source::Image { operation, input, artifact } => {
            let operation = OperationId::new(operation.into_bytes())?;
            let Some(entry) = record
                .images()
                .entries()
                .iter()
                .find(|entry| entry.image().operation() == operation)
            else {
                return Err(ControlError::NotFound.into());
            };
            if entry.image().input().as_bytes() != input.as_bytes()
                || entry.image().artifact_bytes() != artifact.as_bytes()
            {
                return Err(ControlError::StaleRevision.into());
            }
            Ok(ContextTarget::Image(operation))
        }
        Source::File { attachment, version } => {
            let attachment = OperationId::new(attachment.into_bytes())?;
            let Some(entry) = record
                .files()
                .entries()
                .iter()
                .find(|entry| entry.file().operation() == attachment)
            else {
                return Err(ControlError::NotFound.into());
            };
            if entry.current().operation().as_bytes() != version.as_bytes() {
                return Err(ControlError::StaleRevision.into());
            }
            Ok(ContextTarget::File(attachment))
        }
        Source::Message { .. } | Source::Invocation { .. } => {
            Err(ControlError::InvalidInput.into())
        }
    }
}
