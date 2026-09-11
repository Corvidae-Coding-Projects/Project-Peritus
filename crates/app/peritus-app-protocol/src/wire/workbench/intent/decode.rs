//! Canonical payload readers for workbench intents.

use crate::{
    ControlOperationId, WorkbenchCheckpointName, WorkbenchContextPreference, WorkbenchIntent,
    WorkbenchLaunchText, WorkbenchPreviewInput,
};
use peritus_codec::{CanonicalReader, CodecError};

use crate::wire::{
    primitive::{invalid, read_digest, read_id, unknown},
    workbench_brief, workbench_checkpoints, workbench_compaction, workbench_context,
    workbench_files, workbench_goal, workbench_images, workbench_init, workbench_inputs,
    workbench_launch, workbench_library, workbench_memory, workbench_permissions, workbench_review,
};

pub(super) fn control(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    Ok(match tag {
        1 => WorkbenchIntent::CreateConversation(super::super::read_title(reader)?),
        2 => WorkbenchIntent::RenameConversation(super::super::read_title(reader)?),
        3 => WorkbenchIntent::PinConversation(reader.read_bool()?),
        4 => WorkbenchIntent::ArchiveConversation(reader.read_bool()?),
        110 => WorkbenchIntent::ForkConversation(workbench_library::read_fork(reader)?),
        5 => WorkbenchIntent::Queue(workbench_inputs::read_intent(reader)?),
        6 => WorkbenchIntent::StartExecution(super::super::read_settings(reader)?),
        _ => return unknown(offset),
    })
}

pub(super) fn preparation(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    Ok(match tag {
        7 => WorkbenchIntent::SetBrief {
            field: workbench_brief::read_field(reader)?,
            text: workbench_inputs::read_text(reader)?,
        },
        13 => WorkbenchIntent::AcceptBriefProposal {
            field: workbench_brief::read_field(reader)?,
            proposal: read_id(reader, ControlOperationId::new)?,
            digest: read_digest(reader)?,
        },
        14 => {
            let source = workbench_context::read_source(reader)?;
            let preference = if reader.read_bool()? {
                Some(match reader.read_u16()? {
                    1 => WorkbenchContextPreference::Pinned,
                    2 => WorkbenchContextPreference::Excluded,
                    _ => return unknown(offset),
                })
            } else {
                None
            };
            WorkbenchIntent::SetContext { source, preference }
        }
        15 => WorkbenchIntent::ApplyCompaction(workbench_compaction::read_preview(reader)?),
        8 => WorkbenchIntent::AttachImage {
            preview: workbench_images::read_preview(reader)?,
            text: workbench_inputs::read_text(reader)?,
        },
        9 => WorkbenchIntent::SelectImage {
            attachment: read_id(reader, ControlOperationId::new)?,
            selected: reader.read_bool()?,
        },
        12 => WorkbenchIntent::AttachFileImport {
            preview: workbench_files::read_import_preview(reader)?,
            text: workbench_inputs::read_text(reader)?,
        },
        10 => WorkbenchIntent::AttachFile {
            preview: workbench_files::read_preview(reader)?,
            text: workbench_inputs::read_text(reader)?,
        },
        11 => WorkbenchIntent::SelectFile {
            attachment: read_id(reader, ControlOperationId::new)?,
            selected: reader.read_bool()?,
        },
        _ => return unknown(offset),
    })
}

pub(super) fn goal(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    Ok(match tag {
        30 => WorkbenchIntent::StartGoal {
            definition: workbench_goal::read_definition(reader)?,
            settings: super::super::read_settings(reader)?,
        },
        31 => WorkbenchIntent::PauseGoal {
            goal: read_id(reader, ControlOperationId::new)?,
            mode: workbench_goal::read_pause(reader)?,
        },
        32 => WorkbenchIntent::ResumeGoal { goal: read_id(reader, ControlOperationId::new)? },
        33 => WorkbenchIntent::UpdateGoalBudget {
            goal: read_id(reader, ControlOperationId::new)?,
            budget: workbench_goal::read_budget(reader)?,
        },
        34 => WorkbenchIntent::ClearGoal { goal: read_id(reader, ControlOperationId::new)? },
        _ => return unknown(offset),
    })
}

pub(super) fn review(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    Ok(match tag {
        50 => WorkbenchIntent::AddReview {
            anchor: workbench_review::read_anchor(reader)?,
            feedback: workbench_review::read_feedback(reader)?,
            message: workbench_inputs::read_text(reader)?,
        },
        51 => WorkbenchIntent::RebindReview {
            comment: read_id(reader, ControlOperationId::new)?,
            anchor: workbench_review::read_anchor(reader)?,
        },
        52 => WorkbenchIntent::DismissReview { comment: read_id(reader, ControlOperationId::new)? },
        _ => return unknown(offset),
    })
}

pub(super) fn preview(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    Ok(match tag {
        70 => WorkbenchIntent::StartPreview(workbench_launch::read_profile(reader)?),
        71 => WorkbenchIntent::InteractPreview {
            launch: read_id(reader, ControlOperationId::new)?,
            input: invalid(offset, WorkbenchPreviewInput::new(reader.read_bytes()?.to_vec()))?,
        },
        72 => WorkbenchIntent::CapturePreview(workbench_launch::read_capture_request(reader)?),
        73 => WorkbenchIntent::StopPreview { launch: read_id(reader, ControlOperationId::new)? },
        74 => WorkbenchIntent::CheckPreviewBehavior {
            launch: read_id(reader, ControlOperationId::new)?,
            observed: invalid(offset, WorkbenchLaunchText::new(reader.read_str()?.to_owned()))?,
            note: workbench_inputs::read_text(reader)?,
        },
        75 => WorkbenchIntent::AddArtifactFeedback {
            capture: read_id(reader, ControlOperationId::new)?,
            feedback: workbench_review::read_feedback(reader)?,
            message: workbench_inputs::read_text(reader)?,
            region: workbench_launch::read_region(reader)?,
        },
        _ => return unknown(offset),
    })
}

pub(super) fn checkpoint(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    Ok(match tag {
        90 => WorkbenchIntent::CreateCheckpoint(invalid(
            offset,
            WorkbenchCheckpointName::new(reader.read_str()?.to_owned()),
        )?),
        91 => WorkbenchIntent::ApplyRewind(workbench_checkpoints::read_preview(reader)?),
        _ => return unknown(offset),
    })
}

pub(super) fn policy(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    Ok(match tag {
        130 => WorkbenchIntent::SetPermissions(workbench_permissions::read_change(reader)?),
        131 => WorkbenchIntent::SaveGuidance(workbench_memory::read_save(reader)?),
        132 => WorkbenchIntent::ReviseGuidance(workbench_memory::read_revision(reader)?),
        133 => WorkbenchIntent::PinGuidance(workbench_memory::read_pin(reader)?),
        134 => WorkbenchIntent::ScopeGuidance(workbench_memory::read_scope_change(reader)?),
        135 => WorkbenchIntent::ForgetGuidance(workbench_memory::read_forget(reader)?),
        136 => WorkbenchIntent::ApplyInitDiff(workbench_init::read_proposal(reader)?),
        _ => return unknown(offset),
    })
}
