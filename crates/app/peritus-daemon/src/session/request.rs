//! Closed A3 request dispatch for one authenticated connection.

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, AppRequestEnvelope,
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, OperationAcknowledgement,
    ShutdownAccepted, ShutdownRequest, encode_app_message,
};
use tokio::sync::mpsc;

use crate::{
    AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery,
    artifact::ArtifactClient,
    command,
    subscription::SubscriptionRegistry,
    terminal::{TerminalBridgeError, TerminalBridgeErrorKind, TerminalRegistry},
};

#[allow(
    clippy::too_many_arguments,
    reason = "authenticated connection-owned registries and negotiated limits stay explicit"
)]
pub(super) async fn handle_request<S>(
    frames: &mut crate::AppFrameStream<S>,
    authority: &AuthorityHandle,
    shutdown: &mpsc::Sender<ShutdownRequest>,
    subscriptions: &mut SubscriptionRegistry,
    artifacts: &mut ArtifactClient,
    terminals: &TerminalRegistry,
    terminal_bindings: &mut Vec<peritus_app_protocol::TerminalBinding>,
    actor_id: peritus_types::ActorId,
    limits: AppProtocolLimits,
    request: AppRequestEnvelope,
) -> Result<(), DaemonError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload = match request.payload() {
        AppRequestPayload::SubmitCommand(value) => {
            AppResponsePayload::CommandResult(command::submit(authority, actor_id, value).await?)
        }
        AppRequestPayload::DaemonStatus => {
            AppResponsePayload::DaemonStatus(authority.status().await?)
        }
        AppRequestPayload::Subscribe(value) => match subscriptions.open(value, limits) {
            Ok(started) => AppResponsePayload::SubscriptionStarted(started),
            Err(error) => {
                AppResponsePayload::Error(AppProtocolError::new(public_error_code(&error), None))
            }
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
                        daemon_error_payload(&error)
                    }
                },
                Err(error) => daemon_error_payload(&error),
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
                Err(error) => daemon_error_payload(&error),
            }
        }
        AppRequestPayload::UploadArtifactChunk(chunk) => {
            match authority
                .upload_artifact_chunk(actor_id, request.context().session_id(), chunk.clone())
                .await
            {
                Ok(()) => acknowledged(&request),
                Err(error) => daemon_error_payload(&error),
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
                Err(error) => daemon_error_payload(&error),
            }
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
                .cancel_artifact_transfer(actor_id, request.context().session_id(), *cancellation)
                .await
            {
                Ok(()) => {
                    artifacts.remove(transfer_id);
                    acknowledged(&request)
                }
                Err(error) => daemon_error_payload(&error),
            }
        }
        AppRequestPayload::AttachTerminal(binding) => {
            match terminals.attach(
                actor_id,
                request.context().session_id(),
                *binding,
                limits.max_terminal_chunk_bytes(),
            ) {
                Ok(_) => {
                    if !terminal_bindings.contains(binding) {
                        terminal_bindings.push(*binding);
                    }
                    AppResponsePayload::TerminalAttached(*binding)
                }
                Err(error) => terminal_error_payload(&error),
            }
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
        AppRequestPayload::CancelTerminal(cancellation) => terminal_operation(
            request.request_id(),
            terminals.cancel(actor_id, request.context().session_id(), *cancellation).map(|_| ()),
        ),
        AppRequestPayload::Shutdown(value) => {
            AppResponsePayload::ShutdownAccepted(ShutdownAccepted::new(*value))
        }
    };
    let shutdown_request = match request.payload() {
        AppRequestPayload::Shutdown(value) => Some(*value),
        _ => None,
    };
    let response = AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        payload,
    );
    frames.write(&AppMessage::Response(response)).await?;
    if let Some(shutdown_request) = shutdown_request {
        shutdown.try_send(shutdown_request).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::ResourceLimit,
                DaemonRecovery::Retry,
                "queue daemon shutdown request",
                "shutdown request queue is unavailable or full",
                error,
            )
        })?;
    }
    Ok(())
}

const fn acknowledged(request: &AppRequestEnvelope) -> AppResponsePayload {
    AppResponsePayload::Acknowledged(OperationAcknowledgement::new(request.request_id()))
}

fn canonical_request_frame(
    request: &AppRequestEnvelope,
    limits: AppProtocolLimits,
) -> Result<Vec<u8>, DaemonError> {
    encode_app_message(&AppMessage::Request(request.clone()), limits).map_err(|error| {
        DaemonError::with_source(
            DaemonErrorCode::InvalidInput,
            DaemonRecovery::CorrectRequest,
            "encode prompt request settlement",
            "accepted prompt request cannot be canonically re-encoded",
            error,
        )
    })
}

fn prompt_error_payload(error: &DaemonError) -> AppResponsePayload {
    let code = match error.operation() {
        "authorize prompt ownership" => AppErrorCode::SessionMismatch,
        "validate prompt freshness" => AppErrorCode::PromptStale,
        "match prompt target" => AppErrorCode::PromptMismatch,
        _ if error.code_kind() == DaemonErrorCode::Unauthorized => AppErrorCode::PromptMismatch,
        _ if error.code_kind() == DaemonErrorCode::RecoveryRequired => AppErrorCode::PromptStale,
        _ => public_error_code(error),
    };
    AppResponsePayload::Error(AppProtocolError::new(code, None))
}

const fn terminal_operation(
    request_id: peritus_app_protocol::RequestId,
    result: Result<(), TerminalBridgeError>,
) -> AppResponsePayload {
    match result {
        Ok(()) => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(request_id)),
        Err(error) => terminal_error_payload(&error),
    }
}

const fn terminal_error_payload(error: &TerminalBridgeError) -> AppResponsePayload {
    let code = match error.kind() {
        TerminalBridgeErrorKind::Capacity => AppErrorCode::LimitExceeded,
        TerminalBridgeErrorKind::Backpressure => AppErrorCode::Backpressure,
        TerminalBridgeErrorKind::OwnershipMismatch => AppErrorCode::SessionMismatch,
        TerminalBridgeErrorKind::InvalidLimit => AppErrorCode::Internal,
        TerminalBridgeErrorKind::ProcessNotRegistered
        | TerminalBridgeErrorKind::RegistrationConflict
        | TerminalBridgeErrorKind::NotPty
        | TerminalBridgeErrorKind::BirthIdentityUnavailable
        | TerminalBridgeErrorKind::ProcessIdentityMismatch
        | TerminalBridgeErrorKind::ReplayUnavailable
        | TerminalBridgeErrorKind::Protocol
        | TerminalBridgeErrorKind::Process
        | TerminalBridgeErrorKind::ProcessNotLive => AppErrorCode::TerminalState,
    };
    AppResponsePayload::Error(AppProtocolError::new(code, None))
}

const fn public_error_code(error: &DaemonError) -> AppErrorCode {
    match error.code_kind() {
        DaemonErrorCode::InvalidInput => AppErrorCode::SubscriptionState,
        DaemonErrorCode::ResourceLimit => AppErrorCode::Backpressure,
        DaemonErrorCode::Unauthorized => AppErrorCode::ReadOnly,
        DaemonErrorCode::NotReady => AppErrorCode::NotReady,
        _ => AppErrorCode::Internal,
    }
}

const fn daemon_error_payload(error: &DaemonError) -> AppResponsePayload {
    AppResponsePayload::Error(AppProtocolError::new(public_error_code(error), None))
}
