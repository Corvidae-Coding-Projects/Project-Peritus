//! Mutation-readiness routing for authenticated Workbench commands.

use super::{
    AppErrorCode, AppProtocolError, AppProtocolLimits, AppRequestEnvelope, AppResponsePayload,
    AuthorityHandle, DaemonError, ProductRunService,
};

pub(super) async fn checkpoint_coverage_page(
    product_runs: &ProductRunService,
    actor_id: peritus_types::ActorId,
    page_request: peritus_app_protocol::WorkbenchCheckpointPageRequest,
    request: &AppRequestEnvelope,
    limits: AppProtocolLimits,
) -> AppResponsePayload {
    let service = product_runs.clone();
    let request = request.clone();
    match tokio::task::spawn_blocking(move || {
        service.checkpoint_coverage_page(actor_id, page_request, &request, limits)
    })
    .await
    {
        Ok(response) => response,
        Err(_) => AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Internal, None)),
    }
}

pub(super) async fn rewind_coverage_page(
    product_runs: &ProductRunService,
    actor_id: peritus_types::ActorId,
    page_request: peritus_app_protocol::WorkbenchRewindPageRequest,
    request: &AppRequestEnvelope,
    limits: AppProtocolLimits,
) -> AppResponsePayload {
    let service = product_runs.clone();
    let request = request.clone();
    match tokio::task::spawn_blocking(move || {
        service.rewind_coverage_page(actor_id, page_request, &request, limits)
    })
    .await
    {
        Ok(response) => response,
        Err(_) => AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Internal, None)),
    }
}

pub(super) async fn respond(
    authority: &AuthorityHandle,
    product_runs: &ProductRunService,
    actor_id: peritus_types::ActorId,
    limits: AppProtocolLimits,
    checkpoint_pages: bool,
    request: &AppRequestEnvelope,
    command: &peritus_app_protocol::WorkbenchCommand,
) -> Result<AppResponsePayload, DaemonError> {
    if !authority.status().await?.mutation_ready() {
        return Ok(AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::ReadOnly, None)));
    }
    let response =
        if matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::ConfirmRewind(_)) {
            if checkpoint_pages {
                product_runs
                    .apply_paged_workbench_rewind(
                        actor_id,
                        request.context().session_id(),
                        command,
                        request,
                        limits,
                    )
                    .await
            } else {
                AppResponsePayload::Error(AppProtocolError::new(
                    AppErrorCode::MissingRequiredFeature,
                    None,
                ))
            }
        } else if matches!(
            command.intent(),
            peritus_app_protocol::WorkbenchIntent::ApplyInitDiff(_)
                | peritus_app_protocol::WorkbenchIntent::ApplyInitArtifact(_)
                | peritus_app_protocol::WorkbenchIntent::ApplyRewind(_)
        ) {
            product_runs
                .workbench_folder_command(actor_id, request.context().session_id(), command)
                .await
        } else if matches!(
            command.intent(),
            peritus_app_protocol::WorkbenchIntent::AttachImage { .. }
        ) {
            product_runs.confirm_workbench_image(authority, actor_id, command).await
        } else if matches!(
            command.intent(),
            peritus_app_protocol::WorkbenchIntent::AttachFileImport { .. }
                | peritus_app_protocol::WorkbenchIntent::EnqueueMessage { .. }
                | peritus_app_protocol::WorkbenchIntent::EnqueueMessageBundle { .. }
        ) {
            product_runs.confirm_workbench_file_import(authority, actor_id, command).await
        } else if matches!(
            command.intent(),
            peritus_app_protocol::WorkbenchIntent::AttachFile { .. }
        ) {
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
    match response {
        AppResponsePayload::WorkbenchCheckpoint(receipt)
            if checkpoint_pages
                && matches!(
                    command.intent(),
                    peritus_app_protocol::WorkbenchIntent::CreateCheckpoint(_)
                ) =>
        {
            Ok(product_runs.first_checkpoint_coverage_page(&receipt, request, limits))
        }
        response => Ok(response),
    }
}
