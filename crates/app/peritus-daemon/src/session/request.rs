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
        blocking::authorize(product_runs, actor_id, request.payload()).await
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
                media::respond(
                    authority,
                    artifacts,
                    product_runs,
                    actor_id,
                    limits,
                    context.supports(
                        peritus_app_protocol::WellKnownProtocolFeature::WorkbenchRequestSources,
                    ),
                    &request,
                )
                .await
            }
            AppRequestPayload::InspectWorkbenchCheckpoint(value) => {
                query::respond(product_runs, actor_id, query::Request::Checkpoint(*value)).await
            }
            AppRequestPayload::PreviewWorkbenchRewind(value) => {
                product_runs.preview_workbench_rewind(actor_id, value).await
            }
            AppRequestPayload::QueryWorkbenchMemory(query) => {
                query::respond(product_runs, actor_id, query::Request::Memory(*query)).await
            }
            AppRequestPayload::DiscoverInit(value) => {
                query::respond(product_runs, actor_id, query::Request::Init(*value)).await
            }
            AppRequestPayload::QueryWorkbenchPermissions(query) => {
                query::respond(product_runs, actor_id, query::Request::Permissions(*query)).await
            }
            AppRequestPayload::PreviewWorkbenchCompaction(value) => {
                query::respond(product_runs, actor_id, query::Request::Compaction(value.clone()))
                    .await
            }
            AppRequestPayload::QueryWorkbenchPreview(query) => {
                query::respond(product_runs, actor_id, query::Request::Preview(*query)).await
            }
            AppRequestPayload::QueryWorkbenchResult(query) => {
                query::respond(product_runs, actor_id, query::Request::Result(*query)).await
            }
            AppRequestPayload::QueryWorkbenchReview(query) => {
                query::respond(product_runs, actor_id, query::Request::Review(*query)).await
            }
            AppRequestPayload::QueryConversationLibrary(query) => {
                query::respond(product_runs, actor_id, query::Request::Library(query.clone())).await
            }
            AppRequestPayload::QueryWorkbenchContext(query) => {
                query::respond(product_runs, actor_id, query::Request::Context(*query)).await
            }
            AppRequestPayload::QueryWorkbenchBrief(query) => {
                query::respond(
                    product_runs,
                    actor_id,
                    query::Request::Brief(
                        *query,
                        context.supports(
                            peritus_app_protocol::WellKnownProtocolFeature::WorkbenchRequestSources,
                        ),
                    ),
                )
                .await
            }
            AppRequestPayload::QueryWorkbenchGoal(query) => {
                query::respond(product_runs, actor_id, query::Request::Goal(*query)).await
            }
            AppRequestPayload::QueryWorkbenchQueue(query) => {
                query::respond(
                    product_runs,
                    actor_id,
                    query::Request::Queue(
                        *query,
                        context.supports(
                            peritus_app_protocol::WellKnownProtocolFeature::WorkbenchRequestSources,
                        ),
                    ),
                )
                .await
            }
            AppRequestPayload::WorkbenchCommand(command) => {
                workbench::respond(
                    authority,
                    product_runs,
                    actor_id,
                    limits,
                    &request,
                    command,
                    context,
                )
                .await?
            }
            AppRequestPayload::ContinueWorkbenchExecution(_) => {
                AppResponsePayload::Error(AppProtocolError::new(
                    AppErrorCode::MissingRequiredFeature,
                    None,
                ))
            }
            AppRequestPayload::QueryWorkbenchContinuationAdmission(command) => {
                query::respond(
                    product_runs,
                    actor_id,
                    query::Request::ContinuationAdmission(command.clone()),
                )
                .await
            }
            AppRequestPayload::QueryWorkbenchExecution(query) => {
                query::respond(product_runs, actor_id, query::Request::Execution(*query)).await
            }
            AppRequestPayload::QueryInteractionBinding(query) => {
                query::respond(product_runs, actor_id, query::Request::Binding(*query)).await
            }
            AppRequestPayload::QueryWorkbench(query) => {
                query::respond(product_runs, actor_id, query::Request::Workbench(*query)).await
            }
            AppRequestPayload::QueryWorkbenchReceipt(command) => {
                query::respond(product_runs, actor_id, query::Request::Receipt(command.clone()))
                    .await
            }
            AppRequestPayload::Doctor(value) => {
                query::respond(product_runs, actor_id, query::Request::Doctor(*value)).await
            }
            AppRequestPayload::SubmitCommand(value) => match command::submit_with_capacity(
                authority,
                actor_id,
                value,
                limits.max_active_idempotency_slots(),
            )
            .await
            {
                Ok(result) => AppResponsePayload::CommandResult(result),
                Err(error) => command_error_payload(&error),
            },
            AppRequestPayload::DaemonStatus => {
                AppResponsePayload::DaemonStatus(
                    authority.status().await?.constrained(limits.max_diagnostic_bytes()),
                )
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
                    .open_artifact_with_media_type_limit(
                        actor_id,
                        request.context().session_id(),
                        *value,
                        limits.max_artifact_chunk_bytes(),
                        limits.codec().max_string_bytes,
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
                let paged_response = context.supports(
                    peritus_app_protocol::WellKnownProtocolFeature::HarnessImprovementPages,
                );
                match product_runs
                    .improvements_with_representation(actor_id, value, paged_response)
                    .await
                {
                    Ok(payload) => payload,
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
                query::respond(product_runs, actor_id, query::Request::Interaction(*value)).await
            }
            AppRequestPayload::QueryInteractionPage(value) => {
                query::respond(product_runs, actor_id, query::Request::ActivityPage(*value)).await
            }
            AppRequestPayload::QueryModels(value) => {
                match product_runs.query_models(*value).await {
                    Ok(catalog) => AppResponsePayload::Models(catalog),
                    Err(error) => product_run_error(error),
                }
            }
            AppRequestPayload::ControlProductRun(value) => {
                let response = product_runs
                    .control_authenticated(actor_id, request.request_id(), *value)
                    .await;
                if context.supports(
                    peritus_app_protocol::WellKnownProtocolFeature::ProductRunArtifacts,
                ) && !matches!(response, AppResponsePayload::Error(_))
                {
                    match product_runs.query_run_references(
                        peritus_app_protocol::ProductRunReferenceQuery::exact(value.run_id()),
                    ) {
                        Ok(page) => AppResponsePayload::ProductRunReferencePage(page),
                        Err(error) => product_run_error(error),
                    }
                } else {
                    response
                }
            }
            AppRequestPayload::QueryProductRunObservations(value) => {
                query::respond(product_runs, actor_id, query::Request::Observations(*value)).await
            }
            AppRequestPayload::QueryProductRunPage(value) => {
                query::respond(product_runs, actor_id, query::Request::RunPage(*value)).await
            }
            AppRequestPayload::QueryProductRunReferences(value) => {
                query::respond(product_runs, actor_id, query::Request::RunReferences(*value)).await
            }
            AppRequestPayload::QueryProductArtifact(value) => {
                query::respond(
                    product_runs,
                    actor_id,
                    query::Request::ProductArtifact(
                        *value,
                        limits
                            .max_artifact_chunk_bytes()
                            .min(limits.codec().max_opaque_bytes),
                    ),
                )
                .await
            }
            AppRequestPayload::QueryProductDeliverableIndex(value) => {
                query::respond(product_runs, actor_id, query::Request::DeliverableIndex(*value)).await
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
                    .await
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
    let payload = checkpoint_manifests::response(
        payload,
        context
            .supports(peritus_app_protocol::WellKnownProtocolFeature::WorkbenchCheckpointManifests),
    );
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
mod blocking;
mod checkpoint_coverage;
mod checkpoint_manifests;
mod media;
mod query;
mod response;
mod workbench;
use response::terminal_error_payload;
use response::{
    acknowledged, artifact_error_payload, canonical_request_frame, command_error_payload,
    constrain_error_diagnostic, product_run_error, prompt_error_payload, subscription_error_code,
    terminal_operation,
};
