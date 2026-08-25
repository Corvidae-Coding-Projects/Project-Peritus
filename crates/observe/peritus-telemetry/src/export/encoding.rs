//! Canonical redaction-safe hashing representation for export records and batches.

use peritus_trace::{
    CausalBinding, DiagnosticCode, SafeAttribute, SafeAttributeKey, SafeAttributeValue, SpanKind,
    SpanOutcome, StatusCode,
};
use peritus_types::Sha256Digest;

use super::{ExportItem, ExportRecord, ExportStreamId};
use crate::{MetricPoint, OtelEvent, OtelSpan, TelemetryError, TelemetryErrorKind};

pub(super) fn encode_record(record: &ExportRecord) -> Result<Vec<u8>, TelemetryError> {
    let mut bytes = b"PERITUS-C7-EXPORT-RECORD-V1\0".to_vec();
    match record {
        ExportRecord::Span(span) => encode_span(&mut bytes, span)?,
        ExportRecord::Event(event) => encode_event(&mut bytes, event)?,
        ExportRecord::Metric(metric) => encode_metric(&mut bytes, *metric),
    }
    Ok(bytes)
}

pub(super) fn batch_digest(
    stream: ExportStreamId,
    items: &[ExportItem],
) -> Result<Sha256Digest, TelemetryError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PERITUS-C7-EXPORT-BATCH-V1\0");
    put_len(&mut bytes, items.len())?;
    bytes.extend_from_slice(stream.as_bytes());
    for item in items {
        let record = item.record.canonical_bytes()?;
        bytes.extend_from_slice(&item.sequence.to_be_bytes());
        put_len(&mut bytes, record.len())?;
        bytes.extend_from_slice(&record);
    }
    Ok(peritus_codec::sha256(&bytes))
}

fn encode_span(bytes: &mut Vec<u8>, span: &OtelSpan) -> Result<(), TelemetryError> {
    bytes.push(1);
    bytes.extend_from_slice(span.trace_id().as_bytes());
    bytes.extend_from_slice(span.span_id().as_bytes());
    encode_option_span(bytes, span.parent_span_id());
    bytes.push(span_kind_tag(span.kind()));
    encode_binding(bytes, span.binding());
    encode_time(bytes, span.start());
    encode_time(bytes, span.end());
    bytes.push(outcome_tag(span.outcome()));
    encode_attributes(bytes, span.attributes())?;
    put_len(bytes, span.events().len())?;
    for event in span.events() {
        encode_event(bytes, event)?;
    }
    Ok(())
}

fn encode_event(bytes: &mut Vec<u8>, event: &OtelEvent) -> Result<(), TelemetryError> {
    bytes.push(2);
    bytes.extend_from_slice(event.event_id().as_bytes());
    bytes.extend_from_slice(event.trace_id().as_bytes());
    bytes.extend_from_slice(event.span_id().as_bytes());
    encode_time(bytes, event.time());
    bytes.extend_from_slice(&diagnostic_tag(event.code())?.to_be_bytes());
    encode_attributes(bytes, event.attributes())
}

fn encode_metric(bytes: &mut Vec<u8>, metric: MetricPoint) {
    bytes.push(3);
    bytes.extend_from_slice(&metric.name().tag().to_be_bytes());
    bytes.extend_from_slice(&metric.value().to_be_bytes());
    encode_time(bytes, metric.time());
    bytes.extend_from_slice(metric.trace_id().as_bytes());
}

fn encode_time(bytes: &mut Vec<u8>, time: peritus_trace::ObservedTime) {
    bytes.extend_from_slice(&time.unix_nanos().to_be_bytes());
    bytes.extend_from_slice(&time.monotonic_tick().to_be_bytes());
}

fn encode_option_span(bytes: &mut Vec<u8>, span: Option<peritus_trace::SpanId>) {
    match span {
        Some(span) => {
            bytes.push(1);
            bytes.extend_from_slice(span.as_bytes());
        }
        None => bytes.push(0),
    }
}

fn encode_binding(bytes: &mut Vec<u8>, binding: CausalBinding) {
    bytes.extend_from_slice(binding.session_id().as_bytes());
    encode_option_16(bytes, binding.run_id().map(peritus_types::RunId::into_bytes));
    encode_option_16(bytes, binding.attempt_id().map(peritus_types::AttemptId::into_bytes));
    encode_option_16(bytes, binding.turn_id().map(peritus_types::TurnId::into_bytes));
    encode_option_16(bytes, binding.action_id().map(peritus_types::ActionId::into_bytes));
    encode_option_16(
        bytes,
        binding.provider_profile_id().map(peritus_types::ProviderProfileId::into_bytes),
    );
    encode_option_digest(bytes, binding.tool_descriptor_digest());
    encode_option_16(bytes, binding.gate_id().map(peritus_types::GateId::into_bytes));
    encode_option_16(
        bytes,
        binding.gate_execution_id().map(peritus_types::GateExecutionId::into_bytes),
    );
}

fn encode_attributes(
    bytes: &mut Vec<u8>,
    attributes: &[SafeAttribute],
) -> Result<(), TelemetryError> {
    put_len(bytes, attributes.len())?;
    for attribute in attributes {
        bytes.extend_from_slice(&attribute_key_tag(attribute.key()).to_be_bytes());
        match attribute.value() {
            SafeAttributeValue::Count(value) => {
                bytes.push(1);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            SafeAttributeValue::DurationNanos(value) => {
                bytes.push(2);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            SafeAttributeValue::Identifier(value) => {
                bytes.push(3);
                bytes.extend_from_slice(&value);
            }
            SafeAttributeValue::Digest(value) => {
                bytes.push(4);
                bytes.extend_from_slice(value.as_bytes());
            }
            SafeAttributeValue::Status(value) => {
                bytes.push(5);
                bytes.push(status_tag(value));
            }
            SafeAttributeValue::Vault(value) => {
                bytes.push(6);
                bytes.extend_from_slice(value.digest().as_bytes());
                bytes.extend_from_slice(&value.size().to_be_bytes());
                bytes.extend_from_slice(value.creating_event().as_bytes());
                bytes.extend_from_slice(value.key_reference().as_bytes());
                bytes.extend_from_slice(value.parameters_digest().as_bytes());
            }
        }
    }
    Ok(())
}

fn encode_option_16(bytes: &mut Vec<u8>, value: Option<[u8; 16]>) {
    match value {
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value);
        }
        None => bytes.push(0),
    }
}

fn encode_option_digest(bytes: &mut Vec<u8>, value: Option<Sha256Digest>) {
    match value {
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(value.as_bytes());
        }
        None => bytes.push(0),
    }
}

fn put_len(bytes: &mut Vec<u8>, value: usize) -> Result<(), TelemetryError> {
    let value = u64::try_from(value).map_err(|_| {
        TelemetryError::new(
            TelemetryErrorKind::SequenceOverflow,
            "encode telemetry export",
            "export collection length exceeds the portable representation",
        )
    })?;
    bytes.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

const fn span_kind_tag(value: SpanKind) -> u8 {
    match value {
        SpanKind::AgentTurn => 1,
        SpanKind::Provider => 2,
        SpanKind::Tool => 3,
        SpanKind::Gate => 4,
        SpanKind::Action => 5,
        SpanKind::Recovery => 6,
        SpanKind::Internal => 7,
    }
}

const fn outcome_tag(value: SpanOutcome) -> u8 {
    match value {
        SpanOutcome::Ok => 1,
        SpanOutcome::Error => 2,
        SpanOutcome::Cancelled => 3,
        SpanOutcome::Exhausted => 4,
        SpanOutcome::TimedOut => 5,
        SpanOutcome::Indeterminate => 6,
    }
}

const fn status_tag(value: StatusCode) -> u8 {
    match value {
        StatusCode::Pending => 1,
        StatusCode::Success => 2,
        StatusCode::Failure => 3,
        StatusCode::InfrastructureFailure => 4,
        StatusCode::Cancelled => 5,
        StatusCode::TimedOut => 6,
        StatusCode::Indeterminate => 7,
    }
}

const fn attribute_key_tag(value: SafeAttributeKey) -> u16 {
    match value {
        SafeAttributeKey::ProviderRequest => 1,
        SafeAttributeKey::ToolInvocation => 2,
        SafeAttributeKey::GateEvaluation => 3,
        SafeAttributeKey::BudgetUnits => 4,
        SafeAttributeKey::RetryAttempt => 5,
        SafeAttributeKey::Cancellation => 6,
        SafeAttributeKey::Recovery => 7,
        SafeAttributeKey::CpuNanos => 8,
        SafeAttributeKey::MemoryBytes => 9,
        SafeAttributeKey::InputTokens => 10,
        SafeAttributeKey::OutputTokens => 11,
        SafeAttributeKey::CostMicrounits => 12,
        SafeAttributeKey::QueueDepth => 13,
        SafeAttributeKey::DroppedCount => 14,
        SafeAttributeKey::Status => 15,
        SafeAttributeKey::ArtifactEvidence => 16,
    }
}

const fn diagnostic_tag(value: DiagnosticCode) -> Result<u16, TelemetryError> {
    let tag = match value {
        DiagnosticCode::ProviderRequestStarted => 1,
        DiagnosticCode::ProviderRequestCompleted => 2,
        DiagnosticCode::ProviderRequestFailed => 3,
        DiagnosticCode::ToolDispatchStarted => 10,
        DiagnosticCode::ToolDispatchCompleted => 11,
        DiagnosticCode::ToolDispatchFailed => 12,
        DiagnosticCode::GateStarted => 20,
        DiagnosticCode::GatePassed => 21,
        DiagnosticCode::GateFailed => 22,
        DiagnosticCode::GateBlocked => 23,
        DiagnosticCode::BudgetReserved => 30,
        DiagnosticCode::BudgetCharged => 31,
        DiagnosticCode::BudgetExhausted => 32,
        DiagnosticCode::RetryScheduled => 40,
        DiagnosticCode::CancellationRequested => 50,
        DiagnosticCode::CancellationObserved => 51,
        DiagnosticCode::RecoveryStarted => 60,
        DiagnosticCode::RecoveryCompleted => 61,
        DiagnosticCode::RecoveryFailed => 62,
        DiagnosticCode::ResourceObserved => 70,
        DiagnosticCode::ExporterFailed => 80,
        DiagnosticCode::BufferDropped => 81,
        DiagnosticCode::ShutdownStarted => 90,
        DiagnosticCode::ShutdownCompleted => 91,
        _ => {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "encode telemetry export",
                "diagnostic code is not supported by this exporter schema",
            ));
        }
    };
    Ok(tag)
}
