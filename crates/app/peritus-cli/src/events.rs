use std::{collections::VecDeque, ffi::OsStr, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use peritus_app_protocol::{
    Acknowledgement, AppEventPayload, AppRequestPayload, AppResponsePayload, ControlPayload,
    EventCursor, SubscriptionCancellation, SubscriptionCancellationSource, SubscriptionFilter,
    SubscriptionRequest, WellKnownProtocolFeature, encode_prompt_binding_value,
};
use peritus_types::{EventId, SessionId};

use crate::{
    args::EventArgs,
    client::Client,
    error::CliError,
    id::{generated_id, hex},
    operation::response_error,
    output::Output,
};

pub async fn watch(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: EventArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(
        endpoint,
        session,
        timeout,
        &[WellKnownProtocolFeature::EventSubscriptions],
    )
    .await?;
    let filter = SubscriptionFilter::new(
        arguments.topics,
        client.limits().max_topics(),
        client.limits().codec().max_string_bytes,
    )
    .map_err(|error| CliError::usage(error.to_string()))?;
    let subscription_id = peritus_app_protocol::SubscriptionId::new(generated_id(b"subscription"))
        .map_err(|_| {
            CliError::runtime("create subscription identity", "generated zero identifier")
        })?;
    let request = SubscriptionRequest::new(
        subscription_id,
        filter,
        EventCursor::new(arguments.after),
        arguments.window,
        arguments.snapshot_acceptable,
    )?;
    let identity = Client::new_request_identity()?;
    let response = client.request(identity, AppRequestPayload::Subscribe(request)).await?;
    let AppResponsePayload::SubscriptionStarted(started) = response.payload() else {
        return response_error(response.payload(), "subscription start");
    };
    if started.subscription_id() != subscription_id {
        return Err(CliError::protocol(
            "validate subscription start",
            "daemon established a different subscription identity",
        ));
    }
    output.event(
        serde_json::json!({
            "ok": true,
            "kind": "subscription-started",
            "subscription_id": hex(subscription_id.as_bytes()),
            "after": started.after().get(),
            "maximum_in_flight": started.maximum_in_flight(),
            "session_id": hex(client.context().session_id().as_bytes()),
        }),
        &format!(
            "subscription {} started after cursor {} (window {})",
            hex(subscription_id.as_bytes()),
            started.after().get(),
            started.maximum_in_flight(),
        ),
    )?;

    let retained = usize::try_from(started.maximum_in_flight())
        .unwrap_or_else(|_| client.limits().max_in_flight_events())
        .max(1);
    let mut state = DeliveryState {
        seen: VecDeque::with_capacity(retained),
        retained,
        delivered: 0,
        count: arguments.count,
    };
    stream_events(&mut client, subscription_id, &mut state, output).await
}

struct DeliveryState {
    seen: VecDeque<EventId>,
    retained: usize,
    delivered: u64,
    count: Option<u64>,
}

async fn stream_events(
    client: &mut Client,
    subscription_id: peritus_app_protocol::SubscriptionId,
    state: &mut DeliveryState,
    output: &Output,
) -> Result<(), CliError> {
    loop {
        let event = tokio::select! {
            result = client.read_event() => result?,
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| CliError::connection("listen for interrupt", error.to_string()))?;
                cancel_subscription(client, subscription_id).await?;
                return Err(CliError::interrupted());
            }
        };
        if client.reply_heartbeat(&event).await? {
            continue;
        }
        match event.payload() {
            AppEventPayload::DomainEvent(delivery)
                if delivery.subscription_id() == subscription_id =>
            {
                if handle_delivery(client, subscription_id, state, delivery, output).await? {
                    return Ok(());
                }
            }
            AppEventPayload::SubscriptionGap { subscription_id: id, gap }
                if *id == subscription_id =>
            {
                output.event(
                    serde_json::json!({
                        "ok": false,
                        "kind": "subscription-gap",
                        "subscription_id": hex(id.as_bytes()),
                        "requested": gap.requested().get(),
                        "earliest": gap.earliest().get(),
                        "latest": gap.latest().get(),
                    }),
                    &format!(
                        "subscription gap: requested={}, retained={}..{}",
                        gap.requested().get(),
                        gap.earliest().get(),
                        gap.latest().get(),
                    ),
                )?;
                return Err(CliError::protocol(
                    "stream subscription",
                    "requested cursor is outside retained history",
                ));
            }
            AppEventPayload::Backpressure(value)
                if value.subscription_id() == subscription_id =>
            {
                output.event(
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
                )?;
            }
            AppEventPayload::PromptRequested(binding) => {
                let encoded = encode_prompt_binding_value(binding, client.limits())?;
                output.event(
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
                )?;
            }
            AppEventPayload::Diagnostic(diagnostic) => output.event(
                serde_json::json!({ "ok": true, "kind": "diagnostic", "message": diagnostic.as_str() }),
                diagnostic.as_str(),
            )?,
            AppEventPayload::ReadinessChanged(status) => output.event(
                serde_json::json!({
                    "ok": true,
                    "kind": "readiness-changed",
                    "readiness": format!("{:?}", status.readiness()),
                    "diagnostic": status.diagnostic(),
                }),
                &format!("daemon readiness changed: {:?}", status.readiness()),
            )?,
            _ => {}
        }
    }
}

async fn handle_delivery(
    client: &mut Client,
    subscription_id: peritus_app_protocol::SubscriptionId,
    state: &mut DeliveryState,
    delivery: &peritus_app_protocol::Delivery,
    output: &Output,
) -> Result<bool, CliError> {
    if !state.seen.contains(&delivery.event_id()) {
        render_delivery(output, delivery)?;
        state.seen.push_back(delivery.event_id());
        if state.seen.len() > state.retained {
            state.seen.pop_front();
        }
        state.delivered = state.delivered.checked_add(1).ok_or_else(|| {
            CliError::protocol("count event deliveries", "delivery count overflow")
        })?;
    }
    let correlation = Client::new_request_identity()?.correlation_id;
    client
        .write_control(
            correlation,
            ControlPayload::Acknowledge(Acknowledgement::new(subscription_id, delivery.cursor())),
        )
        .await?;
    if state.count.is_some_and(|count| state.delivered >= count) {
        cancel_subscription(client, subscription_id).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn cancel_subscription(
    client: &mut Client,
    subscription_id: peritus_app_protocol::SubscriptionId,
) -> Result<(), CliError> {
    let correlation = Client::new_request_identity()?.correlation_id;
    let cancellation = SubscriptionCancellation::new(
        subscription_id,
        correlation,
        SubscriptionCancellationSource::Client,
    );
    client.write_control(correlation, ControlPayload::CancelSubscription(cancellation)).await
}

const fn prompt_kind(kind: peritus_app_protocol::PromptKind) -> &'static str {
    match kind {
        peritus_app_protocol::PromptKind::Approval => "approval",
        peritus_app_protocol::PromptKind::UserInput => "user-input",
    }
}

fn render_delivery(
    output: &Output,
    delivery: &peritus_app_protocol::Delivery,
) -> Result<(), CliError> {
    output.event(
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
