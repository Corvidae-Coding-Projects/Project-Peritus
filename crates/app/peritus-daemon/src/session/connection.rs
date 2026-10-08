//! One bounded A3 connection owner with independent read-only provider discovery.

use std::time::Duration;

mod select;
mod terminal_delivery;
#[cfg(test)]
mod terminal_tests;

use peritus_app_protocol::{
    AppErrorCode, AppEventEnvelope, AppEventPayload, AppMessage, AppProtocolError,
    AppProtocolLimits, AppRequestEnvelope, AppResponseEnvelope, AppResponsePayload, ControlPayload,
    NegotiationOutcome,
};
use tokio::sync::{mpsc, watch};
use tokio::time::MissedTickBehavior;

use super::{
    ShutdownCommand, ShutdownEventReceiver,
    heartbeat::ConnectionHeartbeat,
    negotiation::establish,
    request::{CatalogRequests, handle_request},
};
use crate::{
    AuthenticatedConnection, AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery,
    artifact::ArtifactClient,
    product_run::ProductRunService,
    subscription::SubscriptionRegistry,
    terminal::{TerminalBridgeEvent, TerminalRegistry},
};
use terminal_delivery::{terminal_diagnostic, terminal_gap_diagnostic};

pub async fn run_connection(
    connection: AuthenticatedConnection,
    authority: AuthorityHandle,
    terminals: TerminalRegistry,
    product_runs: ProductRunService,
    shutdown: mpsc::Sender<ShutdownCommand>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), DaemonError> {
    let peer = connection.peer();
    let mut frames = connection.into_framed(AppProtocolLimits::PRODUCTION);
    let Some(first) = frames.read_or_eof().await? else { return Ok(()) };
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
    let mut subscriptions = SubscriptionRegistry::new();
    let mut artifacts = ArtifactClient::new();
    let mut terminal_bindings = Vec::new();
    let mut heartbeat = ConnectionHeartbeat::new(context.protocol());
    let mut delivery_tick = tokio::time::interval(Duration::from_millis(100));
    delivery_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut heartbeat_tick = tokio::time::interval(Duration::from_secs(10));
    heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    heartbeat_tick.tick().await;
    let mut resources_released = false;
    let mut catalogs = CatalogRequests::default();

    let result = async {
        loop {
            let action = select::next_action(
                &mut frames,
                &mut stop,
                &mut delivery_tick,
                &mut heartbeat_tick,
                &mut catalogs,
            )
            .await;
            match action {
                ConnectionAction::Stop(changed) => {
                    if changed.is_err() || *stop.borrow() {
                        return Ok(());
                    }
                }
                ConnectionAction::Message(message) => match message? {
                    None => return Ok(()),
                    Some(AppMessage::Request(request)) => {
                        if request.context() != context.protocol() {
                            write_error(&mut frames, &request, AppErrorCode::SessionMismatch)
                                .await?;
                            return Ok(());
                        }
                        if request
                            .payload()
                            .required_workbench_feature()
                            .is_some_and(|feature| !context.supports(feature))
                        {
                            write_error(
                                &mut frames,
                                &request,
                                AppErrorCode::MissingRequiredFeature,
                            )
                            .await?;
                            continue;
                        }
                        if matches!(
                            request.payload(),
                            peritus_app_protocol::AppRequestPayload::QueryModels(_)
                        ) {
                            if !catalogs.start(
                                &product_runs,
                                context.actor_id(),
                                context.limits(),
                                &request,
                            ) {
                                write_error(&mut frames, &request, AppErrorCode::Backpressure)
                                    .await?;
                            }
                            continue;
                        }
                        let shutdown_events = handle_request(
                            &mut frames,
                            &authority,
                            &shutdown,
                            &mut subscriptions,
                            &mut artifacts,
                            &terminals,
                            &product_runs,
                            &mut terminal_bindings,
                            &context,
                            request,
                        )
                        .await?;
                        if let Some(events) = shutdown_events {
                            abandon_artifact_transfers(&authority, &artifacts, &context).await?;
                            terminals.release_attachments(
                                context.actor_id(),
                                context.protocol().session_id(),
                                &terminal_bindings,
                            );
                            terminal_bindings.clear();
                            resources_released = true;
                            relay_shutdown(&mut frames, context.protocol(), events).await?;
                            return Ok(());
                        }
                    }
                    Some(AppMessage::Control(control)) => {
                        if control.context() != context.protocol() {
                            return Err(invalid(
                                "control frame does not match the negotiated context",
                            ));
                        }
                        handle_control(&mut subscriptions, &mut heartbeat, control.payload())?;
                    }
                    Some(_) => {
                        return Err(invalid("post-negotiation frame has an illegal family"));
                    }
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
                        context.limits().max_diagnostic_bytes(),
                        context.supports(
                            peritus_app_protocol::WellKnownProtocolFeature::TerminalFailure,
                        ),
                        context.supports(
                            peritus_app_protocol::WellKnownProtocolFeature::TerminalOutputGaps,
                        ),
                    )
                    .await?;
                }
                ConnectionAction::Catalog(response) => {
                    let response = response.map_err(|error| {
                        DaemonError::with_source(
                            DaemonErrorCode::Transport,
                            DaemonRecovery::Retry,
                            "finish model discovery",
                            "model discovery task did not complete",
                            error,
                        )
                    })?;
                    frames.write(&AppMessage::Response(response)).await?;
                }
                ConnectionAction::Heartbeat => {
                    heartbeat
                        .send(
                            &mut frames,
                            authority.status().await?,
                            context.limits().max_diagnostic_bytes(),
                        )
                        .await?;
                }
            }
        }
    }
    .await;
    catalogs.shutdown().await;
    let cleanup = if resources_released {
        Ok(())
    } else {
        let cleanup = abandon_artifact_transfers(&authority, &artifacts, &context).await;
        terminals.release_attachments(
            context.actor_id(),
            context.protocol().session_id(),
            &terminal_bindings,
        );
        cleanup
    };
    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

async fn abandon_artifact_transfers(
    authority: &AuthorityHandle,
    artifacts: &ArtifactClient,
    context: &super::negotiation::ConnectionContext,
) -> Result<(), DaemonError> {
    let mut after = None;
    loop {
        let batch = artifacts.transfer_batch(after);
        let Some(last) = batch.last().copied() else {
            return Ok(());
        };
        authority
            .abandon_artifact_transfers(
                context.actor_id(),
                context.protocol().session_id(),
                batch,
            )
            .await?;
        after = Some(last);
    }
}

async fn relay_shutdown<S>(
    frames: &mut crate::AppFrameStream<S>,
    context: peritus_app_protocol::ProtocolContext,
    mut events: ShutdownEventReceiver,
) -> Result<(), DaemonError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    while let Some(payload) = events.recv().await {
        let complete = matches!(payload, AppEventPayload::ShutdownComplete(_));
        frames.write(&AppMessage::Event(AppEventEnvelope::new(context, payload))).await?;
        if complete {
            return Ok(());
        }
    }
    Err(DaemonError::new(
        DaemonErrorCode::Transport,
        DaemonRecovery::Retry,
        "relay daemon shutdown",
        "shutdown reporting channel closed before completion",
    ))
}

enum ConnectionAction {
    Stop(Result<(), watch::error::RecvError>),
    Message(Result<Option<AppMessage>, DaemonError>),
    Delivery,
    Heartbeat,
    Catalog(Result<AppResponseEnvelope, tokio::task::JoinError>),
}

async fn pump_terminals<S>(
    frames: &mut crate::AppFrameStream<S>,
    terminals: &TerminalRegistry,
    bindings: &mut Vec<peritus_app_protocol::TerminalBinding>,
    actor_id: peritus_types::ActorId,
    session_id: peritus_types::SessionId,
    context: peritus_app_protocol::ProtocolContext,
    maximum_diagnostic_bytes: usize,
    report_attachment_failure: bool,
    report_output_gaps: bool,
) -> Result<(), DaemonError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut completed = Vec::new();
    for binding in bindings.iter().copied() {
        let events = match terminals.poll(actor_id, session_id, binding) {
            Ok(events) => events,
            Err(error) => {
                terminals.release_attachments(actor_id, session_id, &[binding]);
                completed.push(binding);
                let payload = if report_attachment_failure {
                    AppEventPayload::TerminalUnavailable(binding)
                } else {
                    terminal_diagnostic(&error, maximum_diagnostic_bytes)?
                };
                frames.write(&AppMessage::Event(AppEventEnvelope::new(context, payload))).await?;
                continue;
            }
        };
        for event in events {
            let (payload, terminal) = match event {
                TerminalBridgeEvent::Output(output) => {
                    (AppEventPayload::TerminalOutput(output), None)
                }
                TerminalBridgeEvent::Gap(gap) if report_output_gaps => {
                    (AppEventPayload::TerminalOutputGap(gap), None)
                }
                TerminalBridgeEvent::Gap(gap) => {
                    terminals.release_attachments(actor_id, session_id, &[binding]);
                    completed.push(binding);
                    let payload = if report_attachment_failure {
                        AppEventPayload::TerminalUnavailable(binding)
                    } else {
                        terminal_gap_diagnostic(
                            gap.missing_bytes(),
                            maximum_diagnostic_bytes,
                        )?
                    };
                    frames
                        .write(&AppMessage::Event(AppEventEnvelope::new(context, payload)))
                        .await?;
                    break;
                }
                TerminalBridgeEvent::Exited(exit) => {
                    let process_id = exit.binding().process_id();
                    (AppEventPayload::TerminalExited(exit), Some((exit.binding(), process_id)))
                }
            };
            frames.write(&AppMessage::Event(AppEventEnvelope::new(context, payload))).await?;
            if let Some((binding, process_id)) = terminal {
                completed.push(binding);
                if let Err(error) = terminals.retire(process_id) {
                    frames
                        .write(&AppMessage::Event(AppEventEnvelope::new(
                            context,
                            terminal_diagnostic(&error, maximum_diagnostic_bytes)?,
                        )))
                        .await?;
                }
            }
        }
    }
    bindings.retain(|binding| !completed.contains(binding));
    Ok(())
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
