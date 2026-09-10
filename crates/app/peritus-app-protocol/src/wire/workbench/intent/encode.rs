//! Canonical payload writers for workbench intents.

use crate::{WorkbenchContextPreference, WorkbenchIntent};
use peritus_codec::{CanonicalWriter, CodecError, CodecErrorKind};

use crate::wire::{
    primitive::{write_digest, write_id},
    workbench_brief, workbench_checkpoints, workbench_compaction, workbench_context,
    workbench_files, workbench_goal, workbench_images, workbench_init, workbench_inputs,
    workbench_launch, workbench_library, workbench_memory, workbench_permissions, workbench_review,
};

const fn wrong_domain(writer: &CanonicalWriter) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len())
}

pub(super) fn control(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::CreateConversation(title) | WorkbenchIntent::RenameConversation(title) => {
            writer.write_str(title.as_str())
        }
        WorkbenchIntent::PinConversation(value) | WorkbenchIntent::ArchiveConversation(value) => {
            writer.write_bool(*value)
        }
        WorkbenchIntent::ForkConversation(value) => workbench_library::write_fork(writer, value),
        WorkbenchIntent::Queue(value) => workbench_inputs::write_intent(writer, value),
        WorkbenchIntent::StartExecution(value) => super::super::write_settings(writer, value),
        _ => Err(wrong_domain(writer)),
    }
}

pub(super) fn preparation(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::SetBrief { field, text } => {
            workbench_brief::write_field(writer, *field)?;
            writer.write_str(text.as_str())
        }
        WorkbenchIntent::AcceptBriefProposal { field, proposal, digest } => {
            workbench_brief::write_field(writer, *field)?;
            write_id(writer, proposal.as_bytes())?;
            write_digest(writer, *digest)
        }
        WorkbenchIntent::SetContext { source, preference } => {
            workbench_context::write_source(writer, *source)?;
            writer.write_bool(preference.is_some())?;
            if let Some(preference) = preference {
                writer.write_u16(match preference {
                    WorkbenchContextPreference::Pinned => 1,
                    WorkbenchContextPreference::Excluded => 2,
                })?;
            }
            Ok(())
        }
        WorkbenchIntent::ApplyCompaction(preview) => {
            workbench_compaction::write_preview(writer, preview)
        }
        WorkbenchIntent::AttachImage { preview, text } => {
            workbench_images::write_preview(writer, preview)?;
            writer.write_str(text.as_str())
        }
        WorkbenchIntent::SelectImage { attachment, selected }
        | WorkbenchIntent::SelectFile { attachment, selected } => {
            write_id(writer, attachment.as_bytes())?;
            writer.write_bool(*selected)
        }
        WorkbenchIntent::AttachFile { preview, text } => {
            workbench_files::write_preview(writer, preview)?;
            writer.write_str(text.as_str())
        }
        WorkbenchIntent::AttachFileImport { preview, text } => {
            workbench_files::write_import_preview(writer, preview)?;
            writer.write_str(text.as_str())
        }
        _ => Err(wrong_domain(writer)),
    }
}

pub(super) fn goal(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::StartGoal { definition, settings } => {
            workbench_goal::write_definition(writer, definition)?;
            super::super::write_settings(writer, settings)
        }
        WorkbenchIntent::PauseGoal { goal, mode } => {
            write_id(writer, goal.as_bytes())?;
            workbench_goal::write_pause(writer, *mode)
        }
        WorkbenchIntent::ResumeGoal { goal } | WorkbenchIntent::ClearGoal { goal } => {
            write_id(writer, goal.as_bytes())
        }
        WorkbenchIntent::UpdateGoalBudget { goal, budget } => {
            write_id(writer, goal.as_bytes())?;
            workbench_goal::write_budget(writer, *budget)
        }
        _ => Err(wrong_domain(writer)),
    }
}

pub(super) fn review(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::AddReview { anchor, feedback, message } => {
            workbench_review::write_anchor(writer, anchor)?;
            writer.write_u16(feedback.tag())?;
            writer.write_str(message.as_str())
        }
        WorkbenchIntent::RebindReview { comment, anchor } => {
            write_id(writer, comment.as_bytes())?;
            workbench_review::write_anchor(writer, anchor)
        }
        WorkbenchIntent::DismissReview { comment } => write_id(writer, comment.as_bytes()),
        _ => Err(wrong_domain(writer)),
    }
}

pub(super) fn preview(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::StartPreview(profile) => workbench_launch::write_profile(writer, profile),
        WorkbenchIntent::InteractPreview { launch, input } => {
            write_id(writer, launch.as_bytes())?;
            writer.write_bytes(input.bytes())
        }
        WorkbenchIntent::CapturePreview(request) => {
            workbench_launch::write_capture_request(writer, *request)
        }
        WorkbenchIntent::StopPreview { launch } => write_id(writer, launch.as_bytes()),
        WorkbenchIntent::CheckPreviewBehavior { launch, observed, note } => {
            write_id(writer, launch.as_bytes())?;
            writer.write_str(observed.as_str())?;
            writer.write_str(note.as_str())
        }
        WorkbenchIntent::AddArtifactFeedback { capture, feedback, message, region } => {
            write_id(writer, capture.as_bytes())?;
            writer.write_u16(feedback.tag())?;
            writer.write_str(message.as_str())?;
            workbench_launch::write_region(writer, *region)
        }
        _ => Err(wrong_domain(writer)),
    }
}

pub(super) fn checkpoint(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::CreateCheckpoint(name) => writer.write_str(name.as_str()),
        WorkbenchIntent::ApplyRewind(preview) => {
            workbench_checkpoints::write_preview(writer, preview)
        }
        _ => Err(wrong_domain(writer)),
    }
}

pub(super) fn policy(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::SetPermissions(change) => {
            workbench_permissions::write_change(writer, *change)
        }
        WorkbenchIntent::SaveGuidance(change) => workbench_memory::write_save(writer, change),
        WorkbenchIntent::ReviseGuidance(change) => workbench_memory::write_revision(writer, change),
        WorkbenchIntent::PinGuidance(change) => workbench_memory::write_pin(writer, *change),
        WorkbenchIntent::ScopeGuidance(change) => {
            workbench_memory::write_scope_change(writer, *change)
        }
        WorkbenchIntent::ForgetGuidance(change) => workbench_memory::write_forget(writer, change),
        WorkbenchIntent::ApplyInitDiff(proposal) => {
            workbench_init::write_proposal(writer, proposal)
        }
        _ => Err(wrong_domain(writer)),
    }
}
