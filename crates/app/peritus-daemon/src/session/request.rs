//! Closed A3 request dispatch for one authenticated connection.

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, AppRequestEnvelope,
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ShutdownAccepted,
};
use tokio::sync::mpsc;

mod discovery;
pub(super) use discovery::CatalogRequests;

use super::{ShutdownCommand, ShutdownEventReceiver};

use crate::{
    AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery, artifact::ArtifactClient,
    command, product_run::ProductRunService, subscription::SubscriptionRegistry,
    terminal::TerminalRegistry,
};

#[allow(
    clippy::too_many_arguments,
    reason = "authenticated connection-owned registries and negotiated limits stay explicit"
)]
pub(super) async fn handle_request<S>(
    frames: &mut crate::AppFrameStream<S>,
    authority: &AuthorityHandle,
    shutdown: &mpsc::Sender<ShutdownCommand>,
    subscriptions: &mut SubscriptionRegistry,
    artifacts: &mut ArtifactClient,
    terminals: &TerminalRegistry,
    product_runs: &ProductRunService,
    terminal_bindings: &mut Vec<peritus_app_protocol::TerminalBinding>,
    context: &super::negotiation::ConnectionContext,
    request: AppRequestEnvelope,
) -> Result<Option<ShutdownEventReceiver>, DaemonError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let actor_id = context.actor_id();
    let limits = context.limits();
    let checkpoint_coverage = context
        .supports(peritus_app_protocol::WellKnownProtocolFeature::WorkbenchCheckpointCoverage);
    let payload = if let Err(error) =
        product_runs.authorize_workbench_request(actor_id, request.payload())
    {
        AppResponsePayload::Error(error)
    } else {
        match request.payload() {
            AppRequestPayload::PreviewWorkbenchFileImport(_)
            | AppRequestPayload::BeginWorkbenchFileUpload(_)
            | AppRequestPayload::PreviewWorkbenchFile(_)
            | AppRequestPayload::QueryWorkbenchFiles(_)
            | AppRequestPayload::QueryWorkbenchImages(_)
            | AppRequestPayload::BeginWorkbenchImageUpload(_)
            | AppRequestPayload::PreviewWorkbenchImage(_) => {
                media::respond(authority, artifacts, product_runs, actor_id, limits, &request).await
            }
            AppRequestPayload::InspectWorkbenchCheckpoint(value) => {
                product_runs.inspect_workbench_checkpoint(actor_id, *value)
            }
            AppRequestPayload::PreviewWorkbenchRewind(value) => {
                product_runs.preview_workbench_rewind(actor_id, value).await
            }
            AppRequestPayload::QueryWorkbenchMemory(query) => {
                product_runs.workbench_memory(actor_id, *query)
            }
            AppRequestPayload::DiscoverInit(value) => product_runs.discover_init(actor_id, *value),
            AppRequestPayload::QueryWorkbenchPermissions(query) => {
                product_runs.workbench_permissions(actor_id, *query)
            }
            AppRequestPayload::PreviewWorkbenchCompaction(value) => {
                product_runs.preview_workbench_compaction(actor_id, value)
            }
            AppRequestPayload::QueryWorkbenchPreview(query) => {
                product_runs.workbench_preview(actor_id, *query)
            }
            AppRequestPayload::QueryWorkbenchResult(query) => {
                product_runs.workbench_result(actor_id, *query)
            }
            AppRequestPayload::QueryWorkbenchReview(query) => {
                product_runs.workbench_review(actor_id, *query)
            }
            AppRequestPayload::QueryConversationLibrary(query) => {
                product_runs.conversation_library(actor_id, query)
            }
            AppRequestPayload::QueryWorkbenchContext(query) => {
                product_runs.workbench_context(actor_id, *query)
            }
            AppRequestPayload::QueryWorkbenchBrief(query) => {
                product_runs.workbench_brief(actor_id, *query)
            }
            AppRequestPayload::QueryWorkbenchGoal(query) => {
                product_runs.workbench_goal(actor_id, *query)
            }
            AppRequestPayload::QueryWorkbenchQueue(query) => {
                product_runs.workbench_queue(actor_id, *query)
            }
            AppRequestPayload::WorkbenchCommand(command) => {
                workbench::respond(
                    authority,
                    product_runs,
                    actor_id,
                    limits,
                    &request,
                    command,
                    checkpoint_coverage,
                )
                .await?
            }
            AppRequestPayload::ContinueWorkbenchExecution(query) => {
                product_runs.continue_workbench_execution(actor_id, *query).await
            }
            AppRequestPayload::QueryWorkbenchExecution(query) => {
                product_runs.workbench_execution(actor_id, *query)
            }
            AppRequestPayload::QueryInteractionBinding(query) => product_runs
                .query_interaction_binding(actor_id, *query)
                .map_or_else(product_run_error, AppResponsePayload::InteractionBinding),
            AppRequestPayload::QueryWorkbench(query) => {
                product_runs.workbench_query(actor_id, *query)
            }
            AppRequestPayload::QueryWorkbenchReceipt(command) => {
                product_runs.workbench_receipt(actor_id, command)
            }
            AppRequestPayload::Doctor(value) => match product_runs.doctor(*value) {
                Ok(report) => AppResponsePayload::Doctor(report),
                Err(error) => product_run_error(error),
            },
            AppRequestPayload::SubmitCommand(value) => AppResponsePayload::CommandResult(
                command::submit(authority, actor_id, value).await?,
            ),
            AppRequestPayload::DaemonStatus => {
                AppResponsePayload::DaemonStatus(authority.status().await?)
            }
            AppRequestPayload::Subscribe(value) => match subscriptions.open(value, limits) {
                Ok(started) => AppResponsePayload::SubscriptionStarted(started),
                Err(error) => AppResponsePayload::Error(AppProtocolError::new(
                    subscription_error_code(&error),
                    None,
                )),
            },
            AppRequestPayload::OpenArtifact(value) => {
                match authority
                    .open_artifact(
                        actor_id,
                        request.context().session_id(),
                        *value,
                        limits.max_artifact_chunk_bytes(),
                    )
                    .await
                {
                    Ok(metadata) => match artifacts.register_download(&metadata) {
                        Ok(()) => AppResponsePayload::ArtifactOpened(metadata),
                        Err(error) => {
                            let cancellation = peritus_app_protocol::ArtifactCancellation::new(
                                value.transfer_id(),
                                value.artifact_id(),
                                request.correlation_id(),
                            );
                            let _ = authority
                                .cancel_artifact_transfer(
                                    actor_id,
                                    request.context().session_id(),
                                    cancellation,
                                )
                                .await;
                            artifact_error_payload(&error)
                        }
                    },
                    Err(error) => artifact_error_payload(&error),
                }
            }
            AppRequestPayload::BeginArtifactUpload(metadata) => {
                match authority
                    .begin_artifact_upload(
                        actor_id,
                        request.context().session_id(),
                        metadata.clone(),
                        limits.max_artifact_chunk_bytes(),
                    )
                    .await
                    .and_then(|()| artifacts.register_upload(metadata))
                {
                    Ok(()) => acknowledged(&request),
                    Err(error) => artifact_error_payload(&error),
                }
            }
            AppRequestPayload::UploadArtifactChunk(chunk) => {
                match authority
                    .upload_artifact_chunk(actor_id, request.context().session_id(), chunk.clone())
                    .await
                {
                    Ok(()) => acknowledged(&request),
                    Err(error) => artifact_error_payload(&error),
                }
            }
            AppRequestPayload::CompleteArtifactUpload(completion) => {
                let transfer_id = completion.transfer_id();
                match authority
                    .complete_artifact_upload(actor_id, request.context().session_id(), *completion)
                    .await
                {
                    Ok(()) => {
                        artifacts.remove(transfer_id);
                        acknowledged(&request)
                    }
                    Err(error) => artifact_error_payload(&error),
                }
            }
            AppRequestPayload::Improvements(value) => {
                match product_runs.improvements(actor_id, value).await {
                    Ok(inbox) => AppResponsePayload::Improvements(inbox),
                    Err(error) => product_run_error(error),
                }
            }
            AppRequestPayload::UpdateModels(value) => {
                match product_runs.update_models(actor_id, value).await {
                    Ok(snapshot) => AppResponsePayload::Interaction(snapshot),
                    Err(error) => product_run_error(error),
                }
            }
            AppRequestPayload::QueryInteraction(value) => {
                match product_runs.query_interaction(*value) {
                    Ok(snapshot) => AppResponsePayload::Interaction(snapshot),
                    Err(error) => product_run_error(error),
                }
            }
            AppRequestPayload::QueryModels(value) => {
                match product_runs.query_models(*value).await {
                    Ok(catalog) => AppResponsePayload::Models(catalog),
                    Err(error) => product_run_error(error),
                }
            }
            AppRequestPayload::ControlProductRun(value) => {
                product_runs.control_authenticated(actor_id, request.request_id(), *value).await
            }
            AppRequestPayload::QueryProductRunObservations(value) => {
                response::product_run_observations(product_runs, *value)
            }
            AppRequestPayload::AnswerPrompt(answer) => {
                let prompt_id = answer.correlation().prompt_id();
                let result = match canonical_request_frame(&request, limits) {
                    Ok(frame) => {
                        authority
                            .answer_prompt(
                                actor_id,
                                request.context().session_id(),
                                request.request_id(),
                                answer.clone(),
                                frame,
                            )
                            .await
                    }
                    Err(error) => Err(error),
                };
                match result {
                    Ok(_) => AppResponsePayload::PromptAccepted(prompt_id),
                    Err(error) => prompt_error_payload(&error),
                }
            }
            AppRequestPayload::CancelPrompt(cancellation) => {
                let prompt_id = cancellation.correlation().prompt_id();
                let result = match canonical_request_frame(&request, limits) {
                    Ok(frame) => {
                        authority
                            .cancel_prompt(
                                actor_id,
                                request.context().session_id(),
                                request.request_id(),
                                *cancellation,
                                frame,
                            )
                            .await
                    }
                    Err(error) => Err(error),
                };
                match result {
                    Ok(_) => AppResponsePayload::PromptAccepted(prompt_id),
                    Err(error) => prompt_error_payload(&error),
                }
            }
            AppRequestPayload::CancelArtifact(cancellation) => {
                let transfer_id = cancellation.transfer_id();
                match authority
                    .cancel_artifact_transfer(
                        actor_id,
                        request.context().session_id(),
                        *cancellation,
                    )
                    .await
                {
                    Ok(()) => {
                        artifacts.remove(transfer_id);
                        acknowledged(&request)
                    }
                    Err(error) => artifact_error_payload(&error),
                }
            }
            AppRequestPayload::AttachTerminal(binding) => {
                attachment::attach(product_runs, terminals, context, *binding, terminal_bindings)
            }
            AppRequestPayload::TerminalInput(input) => terminal_operation(
                request.request_id(),
                terminals.input(actor_id, request.context().session_id(), input),
            ),
            AppRequestPayload::TerminalResize(resize) => terminal_operation(
                request.request_id(),
                terminals.resize(actor_id, request.context().session_id(), *resize),
            ),
            AppRequestPayload::DetachTerminal(detach) => {
                match terminals.detach(actor_id, request.context().session_id(), *detach) {
                    Ok(_) => {
                        terminal_bindings.retain(|binding| binding != &detach.binding());
                        acknowledged(&request)
                    }
                    Err(error) => terminal_error_payload(&error),
                }
            }
            AppRequestPayload::CancelTerminal(cancellation) => {
                match terminals.cancel(actor_id, request.context().session_id(), *cancellation) {
                    Ok(_) => {
                        terminal_bindings.retain(|binding| binding != &cancellation.binding());
                        acknowledged(&request)
                    }
                    Err(error) => terminal_error_payload(&error),
                }
            }
            AppRequestPayload::Shutdown(value) => {
                AppResponsePayload::ShutdownAccepted(ShutdownAccepted::new(*value))
            }
        }
    };
    let (shutdown_command, shutdown_events) = match request.payload() {
        AppRequestPayload::Shutdown(value) => {
            let (command, events) = ShutdownCommand::new(*value);
            (Some(command), Some(events))
        }
        _ => (None, None),
    };
    let payload = checkpoint_coverage::response(payload, checkpoint_coverage);
    let payload = constrain_error_diagnostic(payload, limits.max_diagnostic_bytes());
    let response = AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        payload,
    );
    frames.write(&AppMessage::Response(response)).await?;
    if let Some(shutdown_command) = shutdown_command {
        shutdown.try_send(shutdown_command).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::ResourceLimit,
                DaemonRecovery::Retry,
                "queue daemon shutdown request",
                "shutdown request queue is unavailable or full",
                error,
            )
        })?;
    }
    Ok(shutdown_events)
}

mod attachment;
mod checkpoint_coverage;
mod media;
mod response;
mod workbench;
use response::terminal_error_payload;
use response::{
    acknowledged, artifact_error_payload, canonical_request_frame, constrain_error_diagnostic,
    product_run_error, prompt_error_payload, subscription_error_code, terminal_operation,
};
