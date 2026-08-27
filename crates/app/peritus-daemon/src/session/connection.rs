//! One sequential bounded A3 connection task.

use std::time::Duration;

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, AppRequestEnvelope,
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlPayload, NegotiationOutcome,
    OperationAcknowledgement, ShutdownAccepted, ShutdownRequest,
};
use tokio::sync::{mpsc, watch};
use tokio::time::MissedTickBehavior;

use super::{heartbeat::ConnectionHeartbeat, negotiation::establish};
use crate::{
    AuthenticatedConnection, AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery,
    artifact::ArtifactClient,
    command,
    subscription::SubscriptionRegistry,
    terminal::{
        TerminalBridgeError, TerminalBridgeErrorKind, TerminalBridgeEvent, TerminalRegistry,
    },
};

pub(crate) async fn run_connection(
    connection: AuthenticatedConnection,
    authority: AuthorityHandle,
    terminals: TerminalRegistry,
    shutdown: mpsc::Sender<ShutdownRequest>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), DaemonError> {
    let peer = connection.peer();
    let mut frames = connection.into_framed(AppProtocolLimits::PRODUCTION);
    let first = frames.read().await?;
    let AppMessage::ClientHello(client) = first else {
        return Err(invalid("first application frame is not ClientHello"));
    };
    let establishment = establish(&authority, peer, &client).await?;
    frames.write(&AppMessage::ServerHello(establishment.hello.clone())).await?;
    let Some(context) = establishment.context else {
        debug_assert!(matches!(establishment.hello.outcome(), NegotiationOutcome::Incompatible(_)));
        return Ok(());
    };
    let mut frames = frames.into_inner().into_framed(context.limits());
    let mut subscriptions = SubscriptionRegistry::new(context.limits());
    let mut artifacts = ArtifactClient::new(context.limits());
    let mut terminal_bindings = Vec::new();
    let mut heartbeat = ConnectionHeartbeat::new(context.protocol());
    let mut delivery_tick = tokio::time::interval(Duration::from_millis(100));
    delivery_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut heartbeat_tick = tokio::time::interval(Duration::from_secs(10));
    heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    heartbeat_tick.tick().await;

    let result = async {
      loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() { return Ok(()); }
            }
            message = frames.read() => match message? {
                AppMessage::Request(request) => {
                    if request.context() != context.protocol() {
                        write_error(&mut frames, &request, AppErrorCode::SessionMismatch).await?;
                        return Ok(());
                    }
                    handle_request(
                        &mut frames,
                        &authority,
                        &shutdown,
                        &mut subscriptions,
                        &mut artifacts,
                        &terminals,
                        &mut terminal_bindings,
                        context.actor_id(),
                        context.limits(),
                        request,
                    ).await?;
                }
                AppMessage::Control(control) => {
                    if control.context() != context.protocol() {
                        return Err(invalid("control frame does not match the negotiated context"));
                    }
                    handle_control(&mut subscriptions, &mut heartbeat, control.payload())?;
                }
                _ => return Err(invalid("post-negotiation frame has an illegal family")),
            },
            _ = delivery_tick.tick() => {
                subscriptions.pump(
                    &mut frames,
                    &authority,
                    context.protocol(),
                    context.limits(),
                ).await?;
                artifacts.pump(
                    &mut frames,
                    &authority,
                    context.actor_id(),
                    context.protocol().session_id(),
                    context.protocol(),
                    context.limits(),
                ).await?;
                pump_terminals(
                    &mut frames,
                    &terminals,
                    &terminal_bindings,
                    context.actor_id(),
                    context.protocol().session_id(),
                    context.protocol(),
                ).await?;
            }
            _ = heartbeat_tick.tick() => {
                heartbeat.send(&mut frames, authority.status().await?).await?;
            }
        }
      }
    }.await;
    let cleanup = authority
        .abandon_artifact_transfers(
            context.actor_id(),
            context.protocol().session_id(),
            artifacts.transfer_ids(),
        )
        .await;
    terminals.release_attachments(
        context.actor_id(),
        context.protocol().session_id(),
        &terminal_bindings,
    );
    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

async fn handle_request<S>(
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
                Ok(()) => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(
                    request.request_id(),
                )),
                Err(error) => daemon_error_payload(&error),
            }
        }
        AppRequestPayload::UploadArtifactChunk(chunk) => {
            match authority
                .upload_artifact_chunk(actor_id, request.context().session_id(), chunk.clone())
                .await
            {
                Ok(()) => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(
                    request.request_id(),
                )),
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
                    AppResponsePayload::Acknowledged(OperationAcknowledgement::new(
                        request.request_id(),
                    ))
                }
                Err(error) => daemon_error_payload(&error),
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
                    AppResponsePayload::Acknowledged(OperationAcknowledgement::new(
                        request.request_id(),
                    ))
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
                    AppResponsePayload::Acknowledged(OperationAcknowledgement::new(
                        request.request_id(),
                    ))
                }
                Err(error) => terminal_error_payload(&error),
            }
        }
        AppRequestPayload::CancelTerminal(cancellation) => terminal_operation(
            request.request_id(),
            terminals.cancel(actor_id, request.context().session_id(), *cancellation).map(|_| ()),
        ),
        AppRequestPayload::Shutdown(value) => {
            let accepted = ShutdownAccepted::new(*value);
            AppResponsePayload::ShutdownAccepted(accepted)
        }
        _ => AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::NotReady, None)),
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

async fn pump_terminals<S>(
    frames: &mut crate::AppFrameStream<S>,
    terminals: &TerminalRegistry,
    bindings: &[peritus_app_protocol::TerminalBinding],
    actor_id: peritus_types::ActorId,
    session_id: peritus_types::SessionId,
    context: peritus_app_protocol::ProtocolContext,
) -> Result<(), DaemonError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    for binding in bindings {
        let events =
            terminals.poll(actor_id, session_id, *binding).map_err(terminal_bridge_error)?;
        for event in events {
            let payload = match event {
                TerminalBridgeEvent::Output(output) => {
                    peritus_app_protocol::AppEventPayload::TerminalOutput(output)
                }
                TerminalBridgeEvent::Exited(exit) => {
                    peritus_app_protocol::AppEventPayload::TerminalExited(exit)
                }
            };
            frames
                .write(&AppMessage::Event(peritus_app_protocol::AppEventEnvelope::new(
                    context, payload,
                )))
                .await?;
        }
    }
    Ok(())
}

fn terminal_operation(
    request_id: peritus_app_protocol::RequestId,
    result: Result<(), TerminalBridgeError>,
) -> AppResponsePayload {
    match result {
        Ok(()) => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(request_id)),
        Err(error) => terminal_error_payload(&error),
    }
}

fn terminal_error_payload(error: &TerminalBridgeError) -> AppResponsePayload {
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

fn terminal_bridge_error(error: TerminalBridgeError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "pump terminal attachment",
        "live terminal output became unavailable",
        error,
    )
}

fn handle_control(
    subscriptions: &mut SubscriptionRegistry,
    heartbeat: &mut ConnectionHeartbeat,
    control: &ControlPayload,
) -> Result<(), DaemonError> {
    match control {
        ControlPayload::Acknowledge(value) => subscriptions.acknowledge(*value),
        ControlPayload::CancelSubscription(value) => subscriptions.cancel(*value),
        ControlPayload::Subscription(value) => subscriptions.control(*value),
        ControlPayload::HeartbeatReply(reply) => heartbeat.observe(*reply),
        _ => Err(invalid("control frame has no active connection-owned operation")),
    }
}

fn public_error_code(error: &DaemonError) -> AppErrorCode {
    match error.code_kind() {
        DaemonErrorCode::InvalidInput => AppErrorCode::SubscriptionState,
        DaemonErrorCode::ResourceLimit => AppErrorCode::Backpressure,
        DaemonErrorCode::Unauthorized => AppErrorCode::ReadOnly,
        DaemonErrorCode::NotReady => AppErrorCode::NotReady,
        _ => AppErrorCode::Internal,
    }
}

fn daemon_error_payload(error: &DaemonError) -> AppResponsePayload {
    AppResponsePayload::Error(AppProtocolError::new(public_error_code(error), None))
}

async fn write_error<S>(
    frames: &mut crate::AppFrameStream<S>,
    request: &AppRequestEnvelope,
    code: AppErrorCode,
) -> Result<(), DaemonError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let response = AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::Error(AppProtocolError::new(code, None)),
    );
    frames.write(&AppMessage::Response(response)).await
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "serve application connection",
        detail,
    )
}
