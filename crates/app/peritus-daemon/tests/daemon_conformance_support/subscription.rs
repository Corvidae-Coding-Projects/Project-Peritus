//! Public global-tail subscription scenarios.

use std::io;

use peritus_app_protocol::{
    Acknowledgement, AppEventPayload, AppMessage, AppRequestPayload, AppResponsePayload,
    ControlPayload, Delivery, EventCursor, SubscriptionCancellation,
    SubscriptionCancellationSource, SubscriptionFilter, SubscriptionId, SubscriptionRequest,
    SubscriptionStarted,
};
use peritus_conformance::{
    DaemonConformanceFixture, DaemonConformanceObservation, DaemonSubscriptionObservation,
    DaemonSubscriptionOutcome,
};

use super::command;
use super::process::TestEnvironment;
use super::session::{command_result, fresh_hello};
use super::wire::WireClient;

pub(super) fn resume(
    fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(101)?;
    populate_after(&mut client, fixture.source_cursor())?;
    let subscription = subscription_id(102)?;
    let started = subscribe(
        &mut client,
        subscription,
        fixture.source_cursor(),
        u32::try_from(fixture.maximum_in_flight()).map_err(super::debug_error)?,
        103,
    )?;
    let delivery = receive_domain(&mut client, subscription)?;
    Ok(observation(
        DaemonSubscriptionOutcome::Active,
        started.after().get(),
        Some(delivery.cursor().get()),
        0,
        true,
        false,
        false,
        0,
        0,
        1,
    ))
}

pub(super) fn redelivery(
    _fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(104)?;
    let last = populate_after(&mut client, 3)?;
    let subscription = subscription_id(105)?;
    subscribe(&mut client, subscription, last.saturating_sub(1), 2, 106)?;
    let first = receive_domain(&mut client, subscription)?;
    let second = receive_domain(&mut client, subscription)?;
    let stable = first.event_id() == second.event_id()
        && first.cursor() == second.cursor()
        && first.frame() == second.frame();
    let distinct = first.attempt_id() != second.attempt_id()
        && second.attempt() == first.attempt().saturating_add(1);
    Ok(observation(
        DaemonSubscriptionOutcome::Active,
        last.saturating_sub(1),
        Some(first.cursor().get()),
        1,
        stable,
        distinct,
        false,
        0,
        0,
        1,
    ))
}

pub(super) fn acknowledgement(
    _fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(107)?;
    let last = populate_after(&mut client, 4)?;
    let subscription = subscription_id(108)?;
    subscribe(&mut client, subscription, last.saturating_sub(2), 1, 109)?;
    let first = receive_domain(&mut client, subscription)?;
    client.control(
        110,
        ControlPayload::Acknowledge(Acknowledgement::new(subscription, first.cursor())),
    )?;
    let second = receive_domain(&mut client, subscription)?;
    let released = u64::from(second.cursor() > first.cursor());
    client.control(
        111,
        ControlPayload::CancelSubscription(SubscriptionCancellation::new(
            subscription,
            peritus_app_protocol::CorrelationId::new([111; 16]).map_err(super::debug_error)?,
            SubscriptionCancellationSource::Client,
        )),
    )?;

    let replay_subscription = subscription_id(112)?;
    subscribe(&mut client, replay_subscription, first.cursor().get().saturating_sub(1), 1, 113)?;
    let replayed = receive_domain(&mut client, replay_subscription)?;
    let immutable = replayed.event_id() == first.event_id() && replayed.cursor() == first.cursor();
    Ok(observation(
        DaemonSubscriptionOutcome::Acknowledged,
        last.saturating_sub(2),
        Some(first.cursor().get()),
        0,
        immutable,
        true,
        released == 1,
        released,
        u64::from(!immutable),
        1,
    ))
}

pub(super) fn backpressure(
    _fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(114)?;
    let last = populate_after(&mut client, 4)?;
    let subscription = subscription_id(115)?;
    subscribe(&mut client, subscription, last.saturating_sub(2), 1, 116)?;
    let first = receive_domain(&mut client, subscription)?;
    let maximum = receive_backpressure(&mut client, subscription)?;
    Ok(observation(
        DaemonSubscriptionOutcome::Backpressured,
        last.saturating_sub(2),
        Some(first.cursor().get()),
        0,
        true,
        true,
        false,
        0,
        0,
        u64::from(maximum),
    ))
}

fn populate_after(client: &mut WireClient, target: u64) -> io::Result<u64> {
    let mut last = 0_u64;
    for index in 0_u8..64 {
        if last > target {
            return Ok(last);
        }
        let seed = 120_u8
            .checked_add(index)
            .ok_or_else(|| io::Error::other("subscription fixture seed overflow"))?;
        let key = format!("subscription-event-{index}");
        let fixture = command::genesis(client.context().session_id(), seed, key.as_bytes(), 0x22)?;
        let result = command_result(client, fixture.binding())?
            .ok_or_else(|| io::Error::other("subscription fixture command returned no result"))?;
        let range = result
            .committed_events()
            .ok_or_else(|| io::Error::other("subscription fixture command did not commit"))?;
        last = range.last().get();
    }
    Err(io::Error::other("subscription fixture did not reach requested cursor bound"))
}

fn subscribe(
    client: &mut WireClient,
    subscription: SubscriptionId,
    after: u64,
    maximum_in_flight: u32,
    request_id: u8,
) -> io::Result<SubscriptionStarted> {
    let filter = SubscriptionFilter::new(vec!["system.all".to_owned()], 1, 64)
        .map_err(super::debug_error)?;
    let request = SubscriptionRequest::new(
        subscription,
        filter,
        EventCursor::new(after),
        maximum_in_flight,
        false,
    )
    .map_err(super::debug_error)?;
    let message = client.request(request_id, AppRequestPayload::Subscribe(request))?;
    let AppMessage::Response(response) = message else {
        return Err(io::Error::other("subscription open returned a non-response message"));
    };
    let AppResponsePayload::SubscriptionStarted(started) = response.payload() else {
        return Err(io::Error::other("subscription open was not accepted"));
    };
    Ok(*started)
}

fn receive_domain(client: &mut WireClient, subscription: SubscriptionId) -> io::Result<Delivery> {
    loop {
        let AppMessage::Event(event) = client.read()? else {
            return Err(io::Error::other("subscription emitted a non-event message"));
        };
        if let AppEventPayload::DomainEvent(delivery) = event.payload()
            && delivery.subscription_id() == subscription
        {
            return Ok(delivery.clone());
        }
    }
}

fn receive_backpressure(client: &mut WireClient, subscription: SubscriptionId) -> io::Result<u32> {
    loop {
        let AppMessage::Event(event) = client.read()? else {
            return Err(io::Error::other("subscription emitted a non-event message"));
        };
        if let AppEventPayload::Backpressure(backpressure) = event.payload()
            && backpressure.subscription_id() == subscription
        {
            return Ok(backpressure.maximum_in_flight());
        }
    }
}

fn established(
    seed: u8,
) -> io::Result<(TestEnvironment, super::process::DaemonProcess, WireClient)> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let client = WireClient::establish(process.endpoint(), fresh_hello(seed))?;
    Ok((environment, process, client))
}

fn subscription_id(seed: u8) -> io::Result<SubscriptionId> {
    SubscriptionId::new([seed; 16]).map_err(super::debug_error)
}

#[allow(
    clippy::too_many_arguments,
    reason = "keeps every direct subscription observation explicit at the scenario boundary"
)]
fn observation(
    outcome: DaemonSubscriptionOutcome,
    supplied_cursor: u64,
    first_source_cursor: Option<u64>,
    redeliveries: u64,
    stable_event_identity: bool,
    distinct_attempt_identity: bool,
    acknowledgement_contiguous: bool,
    released_capacity: u64,
    journal_records_deleted: u64,
    peak_in_flight: u64,
) -> DaemonConformanceObservation {
    DaemonConformanceObservation::Subscription(DaemonSubscriptionObservation::new(
        outcome,
        supplied_cursor,
        first_source_cursor,
        redeliveries,
        stable_event_identity,
        distinct_attempt_identity,
        acknowledgement_contiguous,
        released_capacity,
        journal_records_deleted,
        peak_in_flight,
    ))
}
