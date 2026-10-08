//! Stable tag routing for typed workbench intents.

mod decode;
mod encode;

use crate::WorkbenchIntent;
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};

pub(super) fn tag(intent: &WorkbenchIntent) -> u16 {
    match intent {
        WorkbenchIntent::CreateConversation(_) => 1,
        WorkbenchIntent::RenameConversation(_) => 2,
        WorkbenchIntent::PinConversation(_) => 3,
        WorkbenchIntent::ArchiveConversation(_) => 4,
        WorkbenchIntent::Queue(_) => 5,
        WorkbenchIntent::StartExecution(_) => 6,
        WorkbenchIntent::ContinueExecution(_) => 16,
        WorkbenchIntent::SetBrief { .. } => 7,
        WorkbenchIntent::AttachImage { .. } => 8,
        WorkbenchIntent::SelectImage { .. } => 9,
        WorkbenchIntent::AttachFile { .. } => 10,
        WorkbenchIntent::SelectFile { .. } => 11,
        WorkbenchIntent::AttachFileImport { .. } => 12,
        WorkbenchIntent::AttachFileSource { .. } => 17,
        WorkbenchIntent::AcceptBriefProposal { .. } => 13,
        WorkbenchIntent::SetContext { .. } => 14,
        WorkbenchIntent::ApplyCompaction(_) => 15,
        WorkbenchIntent::StartGoal { definition, .. } => {
            if definition.legacy_wire_representable() { 30 } else { 33 }
        }
        WorkbenchIntent::PauseGoal { .. } => 31,
        WorkbenchIntent::ResumeGoal { .. } => 32,
        WorkbenchIntent::ClearGoal { .. } => 34,
        WorkbenchIntent::AddReview { .. } => 50,
        WorkbenchIntent::RebindReview { .. } => 51,
        WorkbenchIntent::DismissReview { .. } => 52,
        WorkbenchIntent::StartPreview(_) => 70,
        WorkbenchIntent::InteractPreview { .. } => 71,
        WorkbenchIntent::CapturePreview(_) => 72,
        WorkbenchIntent::StopPreview { .. } => 73,
        WorkbenchIntent::CheckPreviewBehavior { .. } => 74,
        WorkbenchIntent::AddArtifactFeedback { .. } => 75,
        WorkbenchIntent::CreateCheckpoint(name) => {
            if name.requires_manifest_feature() {
                92
            } else {
                90
            }
        }
        WorkbenchIntent::ApplyRewind(preview) => {
            if preview.requires_manifest_feature() {
                93
            } else {
                91
            }
        }
        WorkbenchIntent::ForkConversation(_) => 110,
        WorkbenchIntent::SetPermissions(_) => 130,
        WorkbenchIntent::SaveGuidance(_) => 131,
        WorkbenchIntent::ReviseGuidance(_) => 132,
        WorkbenchIntent::PinGuidance(_) => 133,
        WorkbenchIntent::ScopeGuidance(_) => 134,
        WorkbenchIntent::ForgetGuidance(_) => 135,
        WorkbenchIntent::ApplyInitDiff(_) => 136,
    }
}

pub(super) fn write(
    writer: &mut CanonicalWriter,
    intent: &WorkbenchIntent,
) -> Result<(), CodecError> {
    match intent {
        WorkbenchIntent::CreateConversation(_)
        | WorkbenchIntent::RenameConversation(_)
        | WorkbenchIntent::PinConversation(_)
        | WorkbenchIntent::ArchiveConversation(_)
        | WorkbenchIntent::ForkConversation(_)
        | WorkbenchIntent::Queue(_)
        | WorkbenchIntent::StartExecution(_)
        | WorkbenchIntent::ContinueExecution(_) => encode::control(writer, intent),
        WorkbenchIntent::SetBrief { .. }
        | WorkbenchIntent::AcceptBriefProposal { .. }
        | WorkbenchIntent::SetContext { .. }
        | WorkbenchIntent::ApplyCompaction(_)
        | WorkbenchIntent::AttachImage { .. }
        | WorkbenchIntent::SelectImage { .. }
        | WorkbenchIntent::AttachFile { .. }
        | WorkbenchIntent::AttachFileImport { .. }
        | WorkbenchIntent::AttachFileSource { .. }
        | WorkbenchIntent::SelectFile { .. } => encode::preparation(writer, intent),
        WorkbenchIntent::StartGoal { .. }
        | WorkbenchIntent::PauseGoal { .. }
        | WorkbenchIntent::ResumeGoal { .. }
        | WorkbenchIntent::ClearGoal { .. } => encode::goal(writer, intent),
        WorkbenchIntent::AddReview { .. }
        | WorkbenchIntent::RebindReview { .. }
        | WorkbenchIntent::DismissReview { .. } => encode::review(writer, intent),
        WorkbenchIntent::StartPreview(_)
        | WorkbenchIntent::InteractPreview { .. }
        | WorkbenchIntent::CapturePreview(_)
        | WorkbenchIntent::StopPreview { .. }
        | WorkbenchIntent::CheckPreviewBehavior { .. }
        | WorkbenchIntent::AddArtifactFeedback { .. } => encode::preview(writer, intent),
        WorkbenchIntent::CreateCheckpoint(_) | WorkbenchIntent::ApplyRewind(_) => {
            encode::checkpoint(writer, intent)
        }
        WorkbenchIntent::SetPermissions(_)
        | WorkbenchIntent::SaveGuidance(_)
        | WorkbenchIntent::ReviseGuidance(_)
        | WorkbenchIntent::PinGuidance(_)
        | WorkbenchIntent::ScopeGuidance(_)
        | WorkbenchIntent::ForgetGuidance(_)
        | WorkbenchIntent::ApplyInitDiff(_) => encode::policy(writer, intent),
    }
}

pub(super) fn read(
    reader: &mut CanonicalReader<'_>,
    tag: u16,
    offset: usize,
) -> Result<WorkbenchIntent, CodecError> {
    match tag {
        1..=6 | 16 | 110 => decode::control(reader, tag, offset),
        7..=15 | 17 => decode::preparation(reader, tag, offset),
        30..=34 => decode::goal(reader, tag, offset),
        50..=52 => decode::review(reader, tag, offset),
        70..=75 => decode::preview(reader, tag, offset),
        90..=93 => decode::checkpoint(reader, tag, offset),
        130..=136 => decode::policy(reader, tag, offset),
        _ => crate::wire::primitive::unknown(offset),
    }
}
