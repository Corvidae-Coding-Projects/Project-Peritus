//! One sequential bounded A3 connection task.

use std::{
    future::{Future, poll_fn},
    task::Poll,
    time::Duration,
};

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, AppRequestEnvelope,
    AppResponseEnvelope, AppResponsePayload, ControlPayload, NegotiationOutcome, ShutdownRequest,
};
use tokio::sync::{mpsc, watch};
use tokio::time::MissedTickBehavior;

use super::{heartbeat::ConnectionHeartbeat, negotiation::establish, request::handle_request};
use crate::{
    AuthenticatedConnection, AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery,
    artifact::ArtifactClient,
    subscription::SubscriptionRegistry,
    terminal::{TerminalBridgeError, TerminalBridgeEvent, TerminalRegistry},
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
        return if matches!(establishment.hello.outcome(), NegotiationOutcome::Incompatible(_)) {
            Ok(())
        } else {
            Err(invalid("compatible negotiation has no established session context"))
        };
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
            let action = {
                let mut changed = Box::pin(stop.changed());
                let mut message = Box::pin(frames.read());
                let mut delivery = Box::pin(delivery_tick.tick());
                let mut heartbeat = Box::pin(heartbeat_tick.tick());
                poll_fn(|poll_context| {
                    if let Poll::Ready(changed) = changed.as_mut().poll(poll_context) {
                        return Poll::Ready(ConnectionAction::Stop(changed));
                    }
                    if let Poll::Ready(message) = message.as_mut().poll(poll_context) {
                        return Poll::Ready(ConnectionAction::Message(message));
                    }
                    if delivery.as_mut().poll(poll_context).is_ready() {
                        return Poll::Ready(ConnectionAction::Delivery);
                    }
                    if heartbeat.as_mut().poll(poll_context).is_ready() {
                        return Poll::Ready(ConnectionAction::Heartbeat);
                    }
                    Poll::Pending
                })
                .await
            };
            match action {
                ConnectionAction::Stop(changed) => {
                    if changed.is_err() || *stop.borrow() {
                        return Ok(());
                    }
                }
                ConnectionAction::Message(message) => match message? {
                    AppMessage::Request(request) => {
                        if request.context() != context.protocol() {
                            write_error(&mut frames, &request, AppErrorCode::SessionMismatch)
                                .await?;
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
                        )
                        .await?;
                    }
                    AppMessage::Control(control) => {
                        if control.context() != context.protocol() {
                            return Err(invalid(
                                "control frame does not match the negotiated context",
                            ));
                        }
                        handle_control(&mut subscriptions, &mut heartbeat, control.payload())?;
                    }
                    _ => return Err(invalid("post-negotiation frame has an illegal family")),
                },
                ConnectionAction::Delivery => {
                    subscriptions
                        .pump(&mut frames, &authority, context.protocol(), context.limits())
                        .await?;
                    artifacts
                        .pump(
                            &mut frames,
                            &authority,
                            context.actor_id(),
                            context.protocol().session_id(),
                            context.protocol(),
                            context.limits(),
                        )
                        .await?;
                    pump_terminals(
                        &mut frames,
                        &terminals,
                        &mut terminal_bindings,
                        context.actor_id(),
                        context.protocol().session_id(),
                        context.protocol(),
                    )
                    .await?;
                }
                ConnectionAction::Heartbeat => {
                    heartbeat.send(&mut frames, authority.status().await?).await?;
                }
            }
        }
    }
    .await;
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

enum ConnectionAction {
    Stop(Result<(), watch::error::RecvError>),
    Message(Result<AppMessage, DaemonError>),
    Delivery,
    Heartbeat,
}

async fn pump_terminals<S>(
    frames: &mut crate::AppFrameStream<S>,
    terminals: &TerminalRegistry,
    bindings: &mut Vec<peritus_app_protocol::TerminalBinding>,
    actor_id: peritus_types::ActorId,
    session_id: peritus_types::SessionId,
    context: peritus_app_protocol::ProtocolContext,
) -> Result<(), DaemonError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut completed = Vec::new();
    for binding in bindings.iter().copied() {
        let events =
            terminals.poll(actor_id, session_id, binding).map_err(terminal_bridge_error)?;
        for event in events {
            let (payload, terminal) = match event {
                TerminalBridgeEvent::Output(output) => {
                    (peritus_app_protocol::AppEventPayload::TerminalOutput(output), None)
                }
                TerminalBridgeEvent::Exited(exit) => {
                    let process_id = exit.binding().process_id();
                    (
                        peritus_app_protocol::AppEventPayload::TerminalExited(exit),
                        Some((exit.binding(), process_id)),
                    )
                }
            };
            frames
                .write(&AppMessage::Event(peritus_app_protocol::AppEventEnvelope::new(
                    context, payload,
                )))
                .await?;
            if let Some((binding, process_id)) = terminal {
                completed.push(binding);
                terminals.retire(process_id).map_err(terminal_bridge_error)?;
            }
        }
    }
    bindings.retain(|binding| !completed.contains(binding));
    Ok(())
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
