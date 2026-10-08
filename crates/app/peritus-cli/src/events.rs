use std::{collections::VecDeque, ffi::OsStr, path::Path, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use peritus_app_protocol::{
    Acknowledgement, AppErrorCode, AppEventEnvelope, AppEventPayload, AppProtocolError,
    AppRequestPayload, AppResponsePayload, ControlPayload, CorrelationId, EventCursor, RequestId,
    SubscriptionCancellation, SubscriptionCancellationSource, SubscriptionFilter, SubscriptionId,
    SubscriptionRequest, WellKnownProtocolFeature, encode_prompt_binding_value,
};
use peritus_types::{EventId, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    args::EventArgs,
    client::{Client, RequestIdentity},
    error::{CliError, ExitCategory},
    id::{hex, parse_hex_id},
    operation::response_error,
    output::{Output, OutputRecord, StreamOutput},
    recovery,
};

const RECONNECT_DELAY: Duration = Duration::from_millis(250);

pub async fn watch(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: EventArgs,
    output: &Output,
) -> Result<(), CliError> {
    let _receipt_lease = recovery::lease(arguments.receipt.as_deref()).await?;
    let stream = output.stream()?;
    let scope = watch_scope_fingerprint(endpoint, &arguments);
    let retained = match arguments.receipt.as_deref() {
        Some(path) => recovery::load::<WatchReceipt>(path).await?,
        None => None,
    };
    if let Some(receipt) = &retained {
        validate_watch_receipt(receipt, &scope, &arguments)?;
        if session.is_some() && session != Some(stored_session(&receipt.session_id)?) {
            return Err(CliError::usage(
                "--session does not match the durable session in --receipt",
            ));
        }
        match receipt.phase {
            WatchPhase::Complete | WatchPhase::Cancelled => {
                return render_terminal_receipt(
                    receipt,
                    arguments.receipt.as_deref(),
                    output,
                    &stream,
                )
                .await;
            }
            WatchPhase::Gap if !receipt.snapshot_acceptable => {
                return Err(subscription_gap_error());
            }
            WatchPhase::Prepared | WatchPhase::Active | WatchPhase::Gap => {}
        }
    }

    let requested_session = retained
        .as_ref()
        .map(|receipt| stored_session(&receipt.session_id))
        .transpose()?
        .or(session);
    let mut client = Client::connect(
        endpoint,
        requested_session,
        timeout,
        &[WellKnownProtocolFeature::EventSubscriptions],
    )
    .await?;
    let filter = subscription_filter(&arguments, &client)?;
    let mut receipt = match retained {
        Some(mut receipt) => {
            if receipt.phase != WatchPhase::Prepared {
                prepare_subscription_request(&mut receipt)?;
                persist(arguments.receipt.as_deref(), &receipt).await?;
            }
            receipt
        }
        None => {
            let seed = Client::new_request_identity()?;
            let subscription_id = SubscriptionId::new(*seed.request_id.as_bytes()).map_err(|_| {
                CliError::runtime("create subscription identity", "generated zero identifier")
            })?;
            let identity = Client::new_request_identity()?;
            let receipt = WatchReceipt {
                version: 1,
                kind: "events-watch".to_owned(),
                scope_sha256: scope,
                session_id: hex(client.context().session_id().as_bytes()),
                subscription_id: hex(subscription_id.as_bytes()),
                request_id: hex(identity.request_id.as_bytes()),
                correlation_id: hex(identity.correlation_id.as_bytes()),
                request_after: arguments.after,
                resume_after: arguments.after,
                last_received: None,
                last_output: None,
                last_acknowledged: None,
                delivered: 0,
                count: arguments.count,
                window: arguments.window,
                snapshot_acceptable: arguments.snapshot_acceptable,
                phase: WatchPhase::Prepared,
                gap: None,
            };
            if let Some(path) = arguments.receipt.as_deref() {
                recovery::create(path, &receipt).await?;
            }
            receipt
        }
    };
    if receipt.session_id != hex(client.context().session_id().as_bytes()) {
        return Err(CliError::protocol(
            "resume event subscription",
            "daemon established a different durable session",
        ));
    }

    let retained_window = usize::try_from(arguments.window)
        .map_err(|_| CliError::usage("--window does not fit this platform"))?;
    if retained_window == 0 || retained_window > client.limits().max_in_flight_events() {
        return Err(CliError::usage(format!(
            "--window must be between 1 and {}",
            client.limits().max_in_flight_events(),
        )));
    }
    let mut state = DeliveryState {
        seen: VecDeque::with_capacity(retained_window),
        retained: retained_window,
        backlog: VecDeque::with_capacity(retained_window),
    };
    loop {
        let start = start_subscription(&mut client, &filter, &receipt).await;
        let started = match start {
            Ok(started) => started,
            Err(error) if reconnectable(&error) => {
                prepare_subscription_request(&mut receipt)?;
                persist(arguments.receipt.as_deref(), &receipt).await?;
                client = reconnect(endpoint, &receipt, timeout).await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        receipt.phase = WatchPhase::Active;
        persist(arguments.receipt.as_deref(), &receipt).await?;
        let ambiguity = receipt
            .last_received
            .zip(receipt.last_output)
            .is_some_and(|(received, output)| received > output)
            || receipt.last_received.is_some() && receipt.last_output.is_none();
        let record = output.event_record(
            serde_json::json!({
                "ok": true,
                "kind": "subscription-started",
                "subscription_id": receipt.subscription_id,
                "after": started.after().get(),
                "maximum_in_flight": started.maximum_in_flight(),
                "session_id": receipt.session_id,
                "receipt": arguments.receipt.as_ref().map(|path| path.display().to_string()),
                "resumed": receipt.delivered > 0 || receipt.request_after != arguments.after,
                "possible_prior_output": ambiguity,
            }),
            &format!(
                "subscription {} started after cursor {} (window {}){}",
                receipt.subscription_id,
                started.after().get(),
                started.maximum_in_flight(),
                if ambiguity {
                    "; prior output may have reached stdout before its receipt checkpoint"
                } else {
                    ""
                },
            ),
        );
        if let Some(error) =
            write_responsive(&mut client, &stream, record, &mut state.backlog).await?
        {
            state.backlog.clear();
            prepare_subscription_request(&mut receipt)?;
            persist(arguments.receipt.as_deref(), &receipt).await?;
            client = reconnect_after(endpoint, &receipt, timeout, error).await?;
            continue;
        }

        match stream_events(
            &mut client,
            &stream,
            output,
            &arguments,
            &mut receipt,
            &mut state,
        )
        .await
        {
            Ok(StreamOutcome::Complete) => {
                receipt.phase = WatchPhase::Complete;
                persist(arguments.receipt.as_deref(), &receipt).await?;
                return Ok(());
            }
            Ok(StreamOutcome::Reconnect(error)) => {
                state.backlog.clear();
                prepare_subscription_request(&mut receipt)?;
                persist(arguments.receipt.as_deref(), &receipt).await?;
                client = reconnect_after(endpoint, &receipt, timeout, error).await?;
            }
            Ok(StreamOutcome::ReplayGap) => {
                state.backlog.clear();
                prepare_subscription_request(&mut receipt)?;
                persist(arguments.receipt.as_deref(), &receipt).await?;
                client = reconnect(endpoint, &receipt, timeout).await?;
            }
            Err(error) if error.category() == ExitCategory::Interrupted => {
                if cancel_subscription(&mut client, subscription_id(&receipt)?)
                    .await
                    .is_ok()
                {
                    receipt.phase = WatchPhase::Cancelled;
                    persist(arguments.receipt.as_deref(), &receipt).await?;
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        }
    }
}

async fn start_subscription(
    client: &mut Client,
    filter: &SubscriptionFilter,
    receipt: &WatchReceipt,
) -> Result<peritus_app_protocol::SubscriptionStarted, CliError> {
    let subscription = subscription_id(receipt)?;
    let request = SubscriptionRequest::new(
        subscription,
        filter.clone(),
        EventCursor::new(receipt.request_after),
        receipt.window,
        receipt.snapshot_acceptable,
    )?;
    let response = client
        .request(
            restored_identity(receipt)?,
            AppRequestPayload::Subscribe(request),
        )
        .await?;
    let AppResponsePayload::SubscriptionStarted(started) = response.payload() else {
        return response_error(response.payload(), "subscription start").and_then(|()| {
            Err(CliError::protocol(
                "subscription start",
                "missing receipt",
            ))
        });
    };
    if started.subscription_id() != subscription
        || started.after().get() != receipt.request_after
        || started.maximum_in_flight() != receipt.window
    {
        return Err(CliError::protocol(
            "validate subscription start",
            "daemon established a different subscription identity, cursor, or window",
        ));
    }
    Ok(*started)
}

async fn stream_events(
    client: &mut Client,
    stream: &StreamOutput,
    output: &Output,
    arguments: &EventArgs,
    receipt: &mut WatchReceipt,
    state: &mut DeliveryState,
) -> Result<StreamOutcome, CliError> {
    let subscription = subscription_id(receipt)?;
    loop {
        let event = if let Some(event) = state.backlog.pop_front() {
            event
        } else {
            tokio::select! {
                result = client.read_event() => match result {
                    Ok(event) => event,
                    Err(error) if reconnectable(&error) => return Ok(StreamOutcome::Reconnect(error)),
                    Err(error) => return Err(error),
                },
                result = tokio::signal::ctrl_c() => {
                    result.map_err(|error| CliError::connection("listen for interrupt", error.to_string()))?;
                    return Err(CliError::interrupted());
                }
            }
        };
        match client.reply_heartbeat(&event).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) if reconnectable(&error) => return Ok(StreamOutcome::Reconnect(error)),
            Err(error) => return Err(error),
        }
        match event.payload() {
            AppEventPayload::DomainEvent(delivery)
                if delivery.subscription_id() == subscription =>
            {
                if delivery.cursor().get() <= receipt.resume_after
                    || state.seen.contains(&delivery.event_id())
                {
                    if let Err(error) = acknowledge(client, subscription, delivery.cursor()).await {
                        if reconnectable(&error) {
                            return Ok(StreamOutcome::Reconnect(error));
                        }
                        return Err(error);
                    }
                    receipt.resume_after = receipt.resume_after.max(delivery.cursor().get());
                    receipt.last_acknowledged = Some(
                        receipt
                            .last_acknowledged
                            .unwrap_or_default()
                            .max(delivery.cursor().get()),
                    );
                    persist(arguments.receipt.as_deref(), receipt).await?;
                    continue;
                }
                if receipt
                    .count
                    .is_some_and(|count| receipt.delivered >= count)
                {
                    continue;
                }
                receipt.last_received = Some(delivery.cursor().get());
                persist(arguments.receipt.as_deref(), receipt).await?;
                let record = render_delivery(output, delivery);
                let disconnected =
                    write_responsive(client, stream, record, &mut state.backlog).await?;
                receipt.last_output = Some(delivery.cursor().get());
                receipt.resume_after = delivery.cursor().get();
                receipt.delivered = receipt.delivered.checked_add(1).ok_or_else(|| {
                    CliError::protocol("count event deliveries", "delivery count overflow")
                })?;
                persist(arguments.receipt.as_deref(), receipt).await?;
                state.seen.push_back(delivery.event_id());
                if state.seen.len() > state.retained {
                    state.seen.pop_front();
                }
                if let Some(error) = disconnected {
                    return Ok(StreamOutcome::Reconnect(error));
                }
                if let Err(error) = acknowledge(client, subscription, delivery.cursor()).await {
                    if reconnectable(&error) {
                        return Ok(StreamOutcome::Reconnect(error));
                    }
                    return Err(error);
                }
                receipt.last_acknowledged = Some(delivery.cursor().get());
                persist(arguments.receipt.as_deref(), receipt).await?;
                if receipt
                    .count
                    .is_some_and(|count| receipt.delivered >= count)
                {
                    let _ = cancel_subscription(client, subscription).await;
                    return Ok(StreamOutcome::Complete);
                }
            }
            AppEventPayload::SubscriptionGap {
                subscription_id: id,
                gap,
            } if *id == subscription => {
                receipt.phase = WatchPhase::Gap;
                receipt.gap = Some(GapReceipt {
                    requested: gap.requested().get(),
                    earliest: gap.earliest().get(),
                    latest: gap.latest().get(),
                });
                persist(arguments.receipt.as_deref(), receipt).await?;
                let record = output.event_record(
                    serde_json::json!({
                        "ok": false,
                        "kind": "subscription-gap",
                        "subscription_id": hex(id.as_bytes()),
                        "requested": gap.requested().get(),
                        "earliest": gap.earliest().get(),
                        "latest": gap.latest().get(),
                        "recovery": if receipt.snapshot_acceptable { "replay-earliest-retained" } else { "receipt-retained" },
                    }),
                    &format!(
                        "subscription gap: requested={}, retained={}..{}",
                        gap.requested().get(),
                        gap.earliest().get(),
                        gap.latest().get(),
                    ),
                );
                let disconnected =
                    write_responsive(client, stream, record, &mut state.backlog).await?;
                if !receipt.snapshot_acceptable {
                    return Err(subscription_gap_error());
                }
                receipt.resume_after = gap.earliest().get().saturating_sub(1);
                receipt.request_after = receipt.resume_after;
                if let Some(error) = disconnected {
                    return Ok(StreamOutcome::Reconnect(error));
                }
                return Ok(StreamOutcome::ReplayGap);
            }
            AppEventPayload::Backpressure(value) if value.subscription_id() == subscription => {
                let record = output.event_record(
                    serde_json::json!({
                        "ok": true,
                        "kind": "subscription-backpressure",
                        "last_delivered": value.last_delivered().get(),
                        "last_acknowledged": value.last_acknowledged().get(),
                        "maximum_in_flight": value.maximum_in_flight(),
                    }),
                    &format!(
                        "subscription backpressure at cursor {} (acknowledged {})",
                        value.last_delivered().get(),
                        value.last_acknowledged().get(),
                    ),
                );
                if let Some(error) =
                    write_responsive(client, stream, record, &mut state.backlog).await?
                {
                    return Ok(StreamOutcome::Reconnect(error));
                }
            }
            AppEventPayload::PromptRequested(binding) => {
                let encoded = encode_prompt_binding_value(binding, client.limits())?;
                let record = output.event_record(
                    serde_json::json!({
                        "ok": true,
                        "kind": "prompt-requested",
                        "prompt_id": hex(binding.correlation().prompt_id().as_bytes()),
                        "prompt_kind": prompt_kind(binding.kind()),
                        "binding_base64": BASE64.encode(encoded),
                    }),
                    &format!(
                        "prompt {} requested ({:?}); use JSON output to capture its exact binding",
                        hex(binding.correlation().prompt_id().as_bytes()),
                        binding.kind(),
                    ),
                );
                if let Some(error) =
                    write_responsive(client, stream, record, &mut state.backlog).await?
                {
                    return Ok(StreamOutcome::Reconnect(error));
                }
            }
            AppEventPayload::Diagnostic(diagnostic) => {
                let record = output.event_record(
                    serde_json::json!({ "ok": true, "kind": "diagnostic", "message": diagnostic.as_str() }),
                    diagnostic.as_str(),
                );
                if let Some(error) =
                    write_responsive(client, stream, record, &mut state.backlog).await?
                {
                    return Ok(StreamOutcome::Reconnect(error));
                }
            }
            AppEventPayload::ReadinessChanged(status) => {
                let record = output.event_record(
                    serde_json::json!({
                        "ok": true,
                        "kind": "readiness-changed",
                        "readiness": format!("{:?}", status.readiness()),
                        "diagnostic": status.diagnostic(),
                    }),
                    &format!("daemon readiness changed: {:?}", status.readiness()),
                );
                if let Some(error) =
                    write_responsive(client, stream, record, &mut state.backlog).await?
                {
                    return Ok(StreamOutcome::Reconnect(error));
                }
            }
            _ => {}
        }
    }
}

async fn write_responsive(
    client: &mut Client,
    stream: &StreamOutput,
    record: OutputRecord,
    backlog: &mut VecDeque<AppEventEnvelope>,
) -> Result<Option<CliError>, CliError> {
    let write = stream.write(record);
    tokio::pin!(write);
    let mut disconnected = None;
    let mut transport_open = true;
    loop {
        tokio::select! {
            result = &mut write => {
                result?;
                return Ok(disconnected);
            }
            result = client.read_event(), if transport_open => {
                match result {
                    Ok(event) => match client.reply_heartbeat(&event).await {
                        Ok(true) => {}
                        Ok(false) if disconnected.is_none() && backlog.len() < backlog.capacity() => {
                            backlog.push_back(event);
                        }
                        Ok(false) if disconnected.is_none() => {
                            disconnected = Some(CliError::connection(
                                "buffer event stream while stdout is blocked",
                                "negotiated in-flight event window was exhausted",
                            ));
                        }
                        Ok(false) => {}
                        Err(error) if reconnectable(&error) => {
                            disconnected = Some(error);
                            transport_open = false;
                        }
                        Err(error) => return Err(error),
                    },
                    Err(error) if reconnectable(&error) => {
                        disconnected = Some(error);
                        transport_open = false;
                    }
                    Err(error) => return Err(error),
                }
            }
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| CliError::connection("listen for interrupt", error.to_string()))?;
                return Err(CliError::interrupted());
            }
        }
    }
}

async fn reconnect_after(
    endpoint: &OsStr,
    receipt: &WatchReceipt,
    timeout: Option<Duration>,
    _cause: CliError,
) -> Result<Client, CliError> {
    reconnect(endpoint, receipt, timeout).await
}

async fn reconnect(
    endpoint: &OsStr,
    receipt: &WatchReceipt,
    timeout: Option<Duration>,
) -> Result<Client, CliError> {
    let session = stored_session(&receipt.session_id)?;
    loop {
        let result = tokio::select! {
            result = Client::connect(
                endpoint,
                Some(session),
                timeout,
                &[WellKnownProtocolFeature::EventSubscriptions],
            ) => result,
            signal = tokio::signal::ctrl_c() => {
                signal.map_err(|error| CliError::connection("listen for interrupt", error.to_string()))?;
                return Err(CliError::interrupted());
            }
        };
        match result {
            Ok(client) => return Ok(client),
            Err(error) if reconnectable(&error) => {
                tokio::select! {
                    () = tokio::time::sleep(RECONNECT_DELAY) => {}
                    signal = tokio::signal::ctrl_c() => {
                        signal.map_err(|failure| CliError::connection("listen for interrupt", failure.to_string()))?;
                        return Err(CliError::interrupted());
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
}

async fn acknowledge(
    client: &mut Client,
    subscription_id: SubscriptionId,
    cursor: EventCursor,
) -> Result<(), CliError> {
    let correlation = Client::new_request_identity()?.correlation_id;
    client
        .write_control(
            correlation,
            ControlPayload::Acknowledge(Acknowledgement::new(subscription_id, cursor)),
        )
        .await
}

async fn cancel_subscription(
    client: &mut Client,
    subscription_id: SubscriptionId,
) -> Result<(), CliError> {
    let correlation = Client::new_request_identity()?.correlation_id;
    let cancellation = SubscriptionCancellation::new(
        subscription_id,
        correlation,
        SubscriptionCancellationSource::Client,
    );
    client
        .write_control(
            correlation,
            ControlPayload::CancelSubscription(cancellation),
        )
        .await
}

fn render_delivery(output: &Output, delivery: &peritus_app_protocol::Delivery) -> OutputRecord {
    output.event_record(
        serde_json::json!({
            "ok": true,
            "kind": "domain-event",
            "subscription_id": hex(delivery.subscription_id().as_bytes()),
            "event_id": hex(delivery.event_id().as_bytes()),
            "cursor": delivery.cursor().get(),
            "attempt_id": hex(delivery.attempt_id().as_bytes()),
            "attempt": delivery.attempt(),
            "frame": {
                "family": delivery.frame().family(),
                "schema_version": delivery.frame().schema_version(),
                "sha256": hex(delivery.frame().digest().as_bytes()),
                "base64": BASE64.encode(delivery.frame().bytes()),
            }
        }),
        &format!(
            "event cursor={} id={} family={} schema={} attempt={}",
            delivery.cursor().get(),
            hex(delivery.event_id().as_bytes()),
            delivery.frame().family(),
            delivery.frame().schema_version(),
            delivery.attempt(),
        ),
    )
}

fn subscription_filter(
    arguments: &EventArgs,
    client: &Client,
) -> Result<SubscriptionFilter, CliError> {
    SubscriptionFilter::new(
        arguments.topics.clone(),
        client.limits().max_topics(),
        client.limits().codec().max_string_bytes,
    )
    .map_err(|error| CliError::usage(error.to_string()))
}

fn prepare_subscription_request(receipt: &mut WatchReceipt) -> Result<(), CliError> {
    let identity = Client::new_request_identity()?;
    receipt.request_id = hex(identity.request_id.as_bytes());
    receipt.correlation_id = hex(identity.correlation_id.as_bytes());
    receipt.request_after = receipt.resume_after;
    receipt.phase = WatchPhase::Prepared;
    Ok(())
}

fn restored_identity(receipt: &WatchReceipt) -> Result<RequestIdentity, CliError> {
    let request = RequestId::new(stored_id(&receipt.request_id, "request")?)
        .map_err(|_| CliError::runtime("validate operation receipt", "invalid request identity"))?;
    let correlation = CorrelationId::new(stored_id(&receipt.correlation_id, "correlation")?)
        .map_err(|_| {
            CliError::runtime(
                "validate operation receipt",
                "invalid correlation identity",
            )
        })?;
    Ok(RequestIdentity::new(request, correlation))
}

fn subscription_id(receipt: &WatchReceipt) -> Result<SubscriptionId, CliError> {
    SubscriptionId::new(stored_id(&receipt.subscription_id, "subscription")?).map_err(|_| {
        CliError::runtime(
            "validate operation receipt",
            "invalid subscription identity",
        )
    })
}

fn stored_session(value: &str) -> Result<SessionId, CliError> {
    SessionId::new(stored_id(value, "session")?).map_err(|_| {
        CliError::runtime(
            "validate operation receipt",
            "invalid session identity",
        )
    })
}

fn stored_id(value: &str, field: &str) -> Result<[u8; 16], CliError> {
    parse_hex_id(value, field).map_err(|_| {
        CliError::runtime(
            "validate operation receipt",
            format!("receipt contains an invalid {field} identity"),
        )
    })
}

fn watch_scope_fingerprint(endpoint: &OsStr, arguments: &EventArgs) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/cli-event-watch-scope/v1\0");
    fingerprint_part(&mut hasher, endpoint.as_encoded_bytes());
    for topic in &arguments.topics {
        fingerprint_part(&mut hasher, topic.as_bytes());
    }
    hasher.update(arguments.after.to_be_bytes());
    hasher.update(arguments.window.to_be_bytes());
    match arguments.count {
        Some(count) => {
            hasher.update([1]);
            hasher.update(count.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.update([u8::from(arguments.snapshot_acceptable)]);
    let digest: [u8; 32] = hasher.finalize().into();
    hex(&digest)
}

fn fingerprint_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn validate_watch_receipt(
    receipt: &WatchReceipt,
    scope: &str,
    arguments: &EventArgs,
) -> Result<(), CliError> {
    if receipt.version != 1 || receipt.kind != "events-watch" {
        return Err(CliError::runtime(
            "validate operation receipt",
            "receipt is not a supported events-watch receipt",
        ));
    }
    if receipt.scope_sha256 != scope {
        return Err(CliError::usage(
            "--receipt belongs to a different event filter, endpoint, or delivery policy",
        ));
    }
    if receipt.count != arguments.count
        || receipt.window != arguments.window
        || receipt.snapshot_acceptable != arguments.snapshot_acceptable
        || receipt.count.is_some_and(|count| receipt.delivered > count)
        || receipt
            .last_acknowledged
            .zip(receipt.last_output)
            .is_some_and(|(acknowledged, output)| acknowledged > output)
    {
        return Err(CliError::runtime(
            "validate operation receipt",
            "events-watch receipt contains inconsistent delivery policy or frontiers",
        ));
    }
    Ok(())
}

async fn persist(path: Option<&Path>, receipt: &WatchReceipt) -> Result<(), CliError> {
    match path {
        Some(path) => recovery::replace(path, receipt).await,
        None => Ok(()),
    }
}

async fn render_terminal_receipt(
    receipt: &WatchReceipt,
    path: Option<&Path>,
    output: &Output,
    stream: &StreamOutput,
) -> Result<(), CliError> {
    let state = match receipt.phase {
        WatchPhase::Complete => "complete",
        WatchPhase::Cancelled => "cancelled",
        _ => "resumable",
    };
    stream
        .write(output.success_record(
            "subscription-receipt",
            serde_json::json!({
                "state": state,
                "subscription_id": receipt.subscription_id,
                "session_id": receipt.session_id,
                "delivered": receipt.delivered,
                "last_output": receipt.last_output,
                "last_acknowledged": receipt.last_acknowledged,
                "receipt": path.map(|path| path.display().to_string()),
            }),
            &format!(
                "subscription {state}; delivered={}; last-output={:?}; last-acknowledged={:?}",
                receipt.delivered, receipt.last_output, receipt.last_acknowledged,
            ),
        ))
        .await
}

fn reconnectable(error: &CliError) -> bool {
    error.category() == ExitCategory::Connection
}

fn subscription_gap_error() -> CliError {
    CliError::rejected(&AppProtocolError::new(
        AppErrorCode::SubscriptionGap,
        None,
    ))
}

const fn prompt_kind(kind: peritus_app_protocol::PromptKind) -> &'static str {
    match kind {
        peritus_app_protocol::PromptKind::Approval => "approval",
        peritus_app_protocol::PromptKind::UserInput => "user-input",
    }
}

struct DeliveryState {
    seen: VecDeque<EventId>,
    retained: usize,
    backlog: VecDeque<AppEventEnvelope>,
}

enum StreamOutcome {
    Complete,
    Reconnect(CliError),
    ReplayGap,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum WatchPhase {
    Prepared,
    Active,
    Gap,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct GapReceipt {
    requested: u64,
    earliest: u64,
    latest: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WatchReceipt {
    version: u32,
    kind: String,
    scope_sha256: String,
    session_id: String,
    subscription_id: String,
    request_id: String,
    correlation_id: String,
    request_after: u64,
    resume_after: u64,
    last_received: Option<u64>,
    last_output: Option<u64>,
    last_acknowledged: Option<u64>,
    delivered: u64,
    count: Option<u64>,
    window: u32,
    snapshot_acceptable: bool,
    phase: WatchPhase,
    gap: Option<GapReceipt>,
}
