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
) -> Result<AppResponsePayload, DaemonError> {
    if !authority.status().await?.mutation_ready() {
        return Ok(AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::ReadOnly, None)));
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
    ) {
        product_runs.confirm_workbench_file_import(authority, actor_id, command).await
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
        product_runs.workbench_command(actor_id, command).await
    };
    Ok(response)
}
