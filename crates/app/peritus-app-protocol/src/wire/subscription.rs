//! Canonical event-subscription value helpers.

use crate::{
    Acknowledgement, AppProtocolLimits, CorrelationId, Delivery, DeliveryAttemptId, EventCursor,
    PauseReason, RegisteredEventFrame, SubscriptionBackpressure, SubscriptionCancellation,
    SubscriptionCancellationSource, SubscriptionFilter, SubscriptionGap, SubscriptionId,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::EventId;

use super::primitive::{invalid, read_id, unknown, write_id};

pub(super) fn write_filter(
    writer: &mut CanonicalWriter,
    value: &SubscriptionFilter,
) -> Result<(), CodecError> {
    writer.write_collection_len(value.topics().len())?;
    for topic in value.topics() {
        writer.write_str(topic)?;
    }
    Ok(())
}

pub(super) fn read_filter(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<SubscriptionFilter, CodecError> {
    let offset = reader.offset();
    let length = reader.read_collection_len()?;
    if length > limits.max_topics() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut topics = Vec::with_capacity(length);
    for _ in 0..length {
        topics.push(reader.read_str()?.to_owned());
    }
    invalid(
        offset,
        SubscriptionFilter::new(topics, limits.max_topics(), limits.codec().max_string_bytes),
    )
}

pub(super) fn write_delivery(
    writer: &mut CanonicalWriter,
    value: &Delivery,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    write_id(writer, value.event_id().as_bytes())?;
    writer.write_u64(value.cursor().get())?;
    write_id(writer, value.attempt_id().as_bytes())?;
    writer.write_u32(value.attempt())?;
    writer.write_bytes(value.frame().bytes())
}

pub(super) fn read_delivery(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<Delivery, CodecError> {
    let offset = reader.offset();
    let subscription_id = read_id(reader, SubscriptionId::new)?;
    let event_id = read_id(reader, EventId::new)?;
    let cursor = EventCursor::new(reader.read_u64()?);
    let attempt_id = read_id(reader, DeliveryAttemptId::new)?;
    let attempt = reader.read_u32()?;
    let frame = invalid(
        reader.offset(),
        RegisteredEventFrame::new(reader.read_bytes_owned()?, limits.codec()),
    )?;
    invalid(offset, Delivery::new(subscription_id, event_id, cursor, attempt_id, attempt, frame))
}

pub(super) fn write_gap(
    writer: &mut CanonicalWriter,
    value: SubscriptionGap,
) -> Result<(), CodecError> {
    writer.write_u64(value.requested().get())?;
    writer.write_u64(value.earliest().get())?;
    writer.write_u64(value.latest().get())
}

pub(super) fn read_gap(reader: &mut CanonicalReader<'_>) -> Result<SubscriptionGap, CodecError> {
    let offset = reader.offset();
    let requested = EventCursor::new(reader.read_u64()?);
    let earliest = EventCursor::new(reader.read_u64()?);
    let latest = EventCursor::new(reader.read_u64()?);
    invalid(offset, SubscriptionGap::new(requested, earliest, latest))
}

pub(super) fn write_backpressure(
    writer: &mut CanonicalWriter,
    value: SubscriptionBackpressure,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    writer.write_u64(value.last_delivered().get())?;
    writer.write_u64(value.last_acknowledged().get())?;
    writer.write_u32(value.maximum_in_flight())
}

pub(super) fn read_backpressure(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<SubscriptionBackpressure, CodecError> {
    let offset = reader.offset();
    let subscription_id = read_id(reader, SubscriptionId::new)?;
    let delivered = EventCursor::new(reader.read_u64()?);
    let acknowledged = EventCursor::new(reader.read_u64()?);
    let maximum = reader.read_u32()?;
    let maximum_usize = usize::try_from(maximum)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))?;
    if maximum_usize > limits.max_in_flight_events() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    if maximum == 0 || acknowledged > delivered {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(SubscriptionBackpressure::new(subscription_id, delivered, acknowledged, maximum))
}

pub(super) fn write_acknowledgement(
    writer: &mut CanonicalWriter,
    value: Acknowledgement,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    writer.write_u64(value.cursor().get())
}

pub(super) fn read_acknowledgement(
    reader: &mut CanonicalReader<'_>,
) -> Result<Acknowledgement, CodecError> {
    Ok(Acknowledgement::new(
        read_id(reader, SubscriptionId::new)?,
        EventCursor::new(reader.read_u64()?),
    ))
}

pub(super) fn write_subscription_cancellation(
    writer: &mut CanonicalWriter,
    value: SubscriptionCancellation,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    write_id(writer, value.correlation_id().as_bytes())?;
    writer.write_u8(match value.source() {
        SubscriptionCancellationSource::Client => 1,
        SubscriptionCancellationSource::Server => 2,
    })
}

pub(super) fn read_subscription_cancellation(
    reader: &mut CanonicalReader<'_>,
) -> Result<SubscriptionCancellation, CodecError> {
    let subscription_id = read_id(reader, SubscriptionId::new)?;
    let correlation_id = read_id(reader, CorrelationId::new)?;
    let offset = reader.offset();
    let source = match reader.read_u8()? {
        1 => SubscriptionCancellationSource::Client,
        2 => SubscriptionCancellationSource::Server,
        _ => return unknown(offset),
    };
    Ok(SubscriptionCancellation::new(subscription_id, correlation_id, source))
}

pub(super) fn write_pause_reason(
    writer: &mut CanonicalWriter,
    value: PauseReason,
) -> Result<(), CodecError> {
    writer.write_u8(match value {
        PauseReason::Client => 1,
        PauseReason::SlowConsumer => 2,
    })
}

pub(super) fn read_pause_reason(
    reader: &mut CanonicalReader<'_>,
) -> Result<PauseReason, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(PauseReason::Client),
        2 => Ok(PauseReason::SlowConsumer),
        _ => unknown(offset),
    }
}
