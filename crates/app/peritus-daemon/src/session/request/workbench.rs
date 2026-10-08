//! Mutation-readiness routing for authenticated Workbench commands.

use super::{
    AppErrorCode, AppProtocolError, AppProtocolLimits, AppRequestEnvelope, AppResponsePayload,
    AuthorityHandle, DaemonError, ProductRunService,
};

pub(super) async fn respond(
    authority: &AuthorityHandle,
    product_runs: &ProductRunService,
    actor_id: peritus_types::ActorId,
    limits: AppProtocolLimits,
    request: &AppRequestEnvelope,
    command: &peritus_app_protocol::WorkbenchCommand,
    context: &crate::session::negotiation::ConnectionContext,
) -> Result<AppResponsePayload, DaemonError> {
    let checkpoint_coverage = context
        .supports(peritus_app_protocol::WellKnownProtocolFeature::WorkbenchCheckpointCoverage);
    let checkpoint_manifests = context
        .supports(peritus_app_protocol::WellKnownProtocolFeature::WorkbenchCheckpointManifests);
    if !authority.status().await?.mutation_ready() {
        return Ok(AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::ReadOnly, None)));
    }
    if !super::checkpoint_coverage::command_supported(command, checkpoint_coverage) {
        return Ok(AppResponsePayload::Error(AppProtocolError::new(
            AppErrorCode::MissingRequiredFeature,
            None,
        )));
    }
    if !super::checkpoint_manifests::command_supported(command, checkpoint_manifests) {
        return Ok(AppResponsePayload::Error(AppProtocolError::new(
            AppErrorCode::MissingRequiredFeature,
            None,
        )));
    }
    let response = if matches!(
        command.intent(),
        peritus_app_protocol::WorkbenchIntent::ApplyInitDiff(_)
            | peritus_app_protocol::WorkbenchIntent::ApplyRewind(_)
    ) {
        product_runs
            .workbench_folder_command(actor_id, request.context().session_id(), command)
            .await
    } else if matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::AttachImage { .. })
    {
        product_runs.confirm_workbench_image(authority, actor_id, command).await
    } else if matches!(
        command.intent(),
        peritus_app_protocol::WorkbenchIntent::AttachFileImport { .. }
            | peritus_app_protocol::WorkbenchIntent::AttachFileSource { .. }
    ) {
        product_runs.confirm_workbench_file_import(authority, actor_id, command).await
    } else if matches!(
        command.intent(),
        peritus_app_protocol::WorkbenchIntent::Queue(
            peritus_app_protocol::WorkbenchQueueIntent::EnqueueSource { .. }
                | peritus_app_protocol::WorkbenchQueueIntent::EditSource { .. }
                | peritus_app_protocol::WorkbenchQueueIntent::CorrectSource { .. }
        )
    ) {
        product_runs.confirm_workbench_request_source(authority, actor_id, command).await
    } else if matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::AttachFile { .. }) {
        product_runs.confirm_workbench_file(actor_id, command).await
    } else if matches!(
        command.intent(),
        peritus_app_protocol::WorkbenchIntent::StartPreview(_)
            | peritus_app_protocol::WorkbenchIntent::InteractPreview { .. }
            | peritus_app_protocol::WorkbenchIntent::CapturePreview(_)
            | peritus_app_protocol::WorkbenchIntent::StopPreview { .. }
            | peritus_app_protocol::WorkbenchIntent::CheckPreviewBehavior { .. }
            | peritus_app_protocol::WorkbenchIntent::AddArtifactFeedback { .. }
    ) {
        product_runs
            .workbench_preview_command(
                authority,
                actor_id,
                request.context().session_id(),
                request.correlation_id(),
                limits.max_artifact_chunk_bytes(),
                command,
            )
            .await
    } else {
        product_runs
            .workbench_command_with_checkpoint_features(
                actor_id,
                command,
                checkpoint_coverage,
                checkpoint_manifests,
            )
            .await
    };
    Ok(response)
}
