//! Session-bound explicit file/image previews and artifact-upload admission.

use super::{
    AppErrorCode, AppProtocolError, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload,
    AppResponsePayload, ArtifactClient, AuthorityHandle, ProductRunService, acknowledged,
    daemon_error_payload,
};

pub(super) async fn respond(
    authority: &AuthorityHandle,
    artifacts: &mut ArtifactClient,
    product_runs: &ProductRunService,
    actor_id: peritus_types::ActorId,
    limits: AppProtocolLimits,
    request: &AppRequestEnvelope,
) -> AppResponsePayload {
    match request.payload() {
        AppRequestPayload::PreviewWorkbenchFileImport(value) => {
            product_runs.preview_workbench_file_import(authority, actor_id, value).await
        }
        AppRequestPayload::BeginWorkbenchFileUpload(value) => {
            let result = product_runs
                .begin_workbench_file_upload(
                    authority,
                    actor_id,
                    request.context().session_id(),
                    value,
                    limits.max_artifact_chunk_bytes(),
                )
                .await;
            match result {
                Ok(()) => match artifacts.register_upload(value.metadata()) {
                    Ok(()) => acknowledged(request),
                    Err(error) => {
                        let cancellation = peritus_app_protocol::ArtifactCancellation::new(
                            value.metadata().transfer_id(),
                            value.metadata().artifact_id(),
                            request.correlation_id(),
                        );
                        let _ = authority
                            .cancel_artifact_transfer(
                                actor_id,
                                request.context().session_id(),
                                cancellation,
                            )
                            .await;
                        daemon_error_payload(&error)
                    }
                },
                Err(error) => AppResponsePayload::Error(error),
            }
        }
        AppRequestPayload::PreviewWorkbenchFile(value) => {
            product_runs.preview_workbench_file(actor_id, value).await
        }
        AppRequestPayload::QueryWorkbenchFiles(query) => {
            product_runs.workbench_files(actor_id, *query)
        }
        AppRequestPayload::QueryWorkbenchImages(query) => {
            product_runs.workbench_images(actor_id, *query)
        }
        AppRequestPayload::BeginWorkbenchImageUpload(value) => {
            match product_runs
                .begin_workbench_image_upload(
                    authority,
                    actor_id,
                    request.context().session_id(),
                    value,
                    limits.max_artifact_chunk_bytes(),
                )
                .await
            {
                Ok(()) => match artifacts.register_upload(value.metadata()) {
                    Ok(()) => acknowledged(request),
                    Err(error) => {
                        let cancellation = peritus_app_protocol::ArtifactCancellation::new(
                            value.metadata().transfer_id(),
                            value.metadata().artifact_id(),
                            request.correlation_id(),
                        );
                        let _ = authority
                            .cancel_artifact_transfer(
                                actor_id,
                                request.context().session_id(),
                                cancellation,
                            )
                            .await;
                        daemon_error_payload(&error)
                    }
                },
                Err(error) => AppResponsePayload::Error(error),
            }
        }
        AppRequestPayload::PreviewWorkbenchImage(value) => {
            product_runs.preview_workbench_image(authority, actor_id, value).await
        }
        _ => AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::MalformedFrame, None)),
    }
}
