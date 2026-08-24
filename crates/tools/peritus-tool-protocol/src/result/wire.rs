//! Deterministic terminal/failure envelope encoding.

use super::{
    BoundedJson, FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability,
    ToolFailure, ToolResult, Truncation,
};

pub(super) fn failure(value: &ToolFailure) -> Vec<u8> {
    let mut bytes = crate::wire::begin(9);
    bytes.push(failure_category_tag(value.category));
    crate::wire::text(&mut bytes, value.code.as_str());
    bytes.push(subsystem_tag(value.subsystem));
    bytes.push(match value.retryability {
        Retryability::Never => 1,
        Retryability::NewAction => 2,
        Retryability::AfterRecovery => 3,
    });
    bytes.push(match value.recovery {
        RecoveryRoute::None => 1,
        RecoveryRoute::Reauthorize => 2,
        RecoveryRoute::ReconcileWorkspace => 3,
        RecoveryRoute::ReconcileProcess => 4,
        RecoveryRoute::RepublishArtifact => 5,
        RecoveryRoute::HumanReview => 6,
    });
    crate::wire::text(&mut bytes, value.detail.as_str());
    bytes
}

pub(super) fn result(value: &ToolResult) -> Vec<u8> {
    let mut bytes = crate::wire::begin(6);
    bytes.extend_from_slice(value.action_id.as_bytes());
    bytes.extend_from_slice(value.descriptor_digest.as_bytes());
    bytes.extend_from_slice(value.prepared_digest.as_bytes());
    bytes.extend_from_slice(value.replay_identity.as_bytes());
    bytes.push(status_tag(value.status));
    append_optional_json(&mut bytes, value.structured.as_ref());
    match &value.failure {
        Some(failure) => {
            bytes.push(1);
            crate::wire::bytes(&mut bytes, &failure.canonical_bytes());
        }
        None => bytes.push(0),
    }
    crate::wire::text(&mut bytes, value.human_rendering.as_str());
    crate::wire::text(&mut bytes, value.model_rendering.as_str());
    let artifact_count = u16::try_from(value.artifacts.len()).unwrap_or(u16::MAX);
    crate::wire::u16_value(&mut bytes, artifact_count);
    for artifact in &value.artifacts {
        crate::wire::bytes(&mut bytes, &artifact.canonical_bytes());
    }
    crate::wire::instant(&mut bytes, value.timing.started_at);
    crate::wire::instant(&mut bytes, value.timing.finished_at);
    bytes.push(truncation_tag(value.truncation.output));
    bytes.push(truncation_tag(value.truncation.model));
    bytes.push(truncation_tag(value.truncation.human));
    crate::wire::u32_value(&mut bytes, value.progress_count);
    bytes
}

fn append_optional_json(bytes: &mut Vec<u8>, value: Option<&BoundedJson>) {
    match value {
        Some(value) => {
            bytes.push(1);
            crate::wire::bytes(bytes, value.canonical_bytes());
        }
        None => bytes.push(0),
    }
}

const fn status_tag(value: ResultStatus) -> u8 {
    match value {
        ResultStatus::Succeeded => 1,
        ResultStatus::Failed => 2,
        ResultStatus::Cancelled => 3,
        ResultStatus::TimedOut => 4,
        ResultStatus::Indeterminate => 5,
    }
}

const fn failure_category_tag(value: FailureCategory) -> u8 {
    match value {
        FailureCategory::Protocol => 1,
        FailureCategory::Authorization => 2,
        FailureCategory::Workspace => 3,
        FailureCategory::Execution => 4,
        FailureCategory::Artifact => 5,
        FailureCategory::Infrastructure => 6,
        FailureCategory::Cancelled => 7,
        FailureCategory::Timeout => 8,
        FailureCategory::Indeterminate => 9,
    }
}

const fn subsystem_tag(value: ResponsibleSubsystem) -> u8 {
    match value {
        ResponsibleSubsystem::Protocol => 1,
        ResponsibleSubsystem::Router => 2,
        ResponsibleSubsystem::Workspace => 3,
        ResponsibleSubsystem::Process => 4,
        ResponsibleSubsystem::Sandbox => 5,
        ResponsibleSubsystem::ArtifactStore => 6,
        ResponsibleSubsystem::Tool => 7,
    }
}

const fn truncation_tag(value: Truncation) -> u8 {
    match value {
        Truncation::Complete => 1,
        Truncation::TailDropped => 2,
        Truncation::HeadDropped => 3,
        Truncation::Windowed => 4,
        Truncation::Indeterminate => 5,
    }
}
