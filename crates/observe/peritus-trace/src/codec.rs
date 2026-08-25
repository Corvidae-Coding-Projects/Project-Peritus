//! Canonical family-60 observation encoding and decoding.

use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{
    ActionId, AttemptId, EventId, GateExecutionId, GateId, ProviderProfileId, RunId, SessionId,
    Sha256Digest, TurnId,
};

use crate::{
    ArtifactVaultReference, CausalBinding, DiagnosticCode, Observation, ObservationKind,
    ObservedTime, RedactedValue, SafeAttribute, SafeAttributeKey, SafeAttributeValue,
    SensitivityClass, SpanId, SpanKind, SpanOutcome, StatusCode, TRACE_OBSERVATION_FAMILY,
    TRACE_OBSERVATION_SCHEMA, TraceId,
};

impl CanonicalEncode for Observation {
    const FAMILY: u16 = TRACE_OBSERVATION_FAMILY;
    const SCHEMA_VERSION: u16 = TRACE_OBSERVATION_SCHEMA;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.event_id().as_bytes())?;
        writer.write_fixed(self.trace_id().as_bytes())?;
        writer.write_fixed(self.span_id().as_bytes())?;
        writer.write_u64(self.span_sequence())?;
        write_option_span(writer, self.parent_span_id())?;
        writer.write_collection_len(self.causal_events().len())?;
        for event in self.causal_events() {
            writer.write_fixed(event.as_bytes())?;
        }
        write_binding(writer, self.binding())?;
        writer.write_u64(self.time().unix_nanos())?;
        writer.write_u64(self.time().monotonic_tick())?;
        write_kind(writer, self.kind())?;
        writer.write_collection_len(self.attributes().len())?;
        for attribute in self.attributes() {
            write_attribute(writer, *attribute)?;
        }
        writer.write_collection_len(self.redactions().len())?;
        for value in self.redactions() {
            write_redaction(writer, *value)?;
        }
        Ok(())
    }
}

impl CanonicalDecode for Observation {
    const FAMILY: u16 = TRACE_OBSERVATION_FAMILY;
    const SCHEMA_VERSION: u16 = TRACE_OBSERVATION_SCHEMA;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let event_id = read_domain_id(reader, EventId::new)?;
        let trace_id = read_trace_id(reader)?;
        let span_id = read_span_id(reader)?;
        let span_sequence = reader.read_u64()?;
        let parent_span_id = read_option_span(reader)?;
        let causal_len = reader.read_collection_len()?;
        let mut causal_events = Vec::with_capacity(causal_len);
        for _ in 0..causal_len {
            causal_events.push(read_domain_id(reader, EventId::new)?);
        }
        let binding = read_binding(reader)?;
        let unix_nanos = reader.read_u64()?;
        let monotonic_tick = reader.read_u64()?;
        let time = ObservedTime::new(unix_nanos, monotonic_tick)
            .map_err(|_| invalid_domain(reader.offset()))?;
        let kind = read_kind(reader)?;
        let attribute_len = reader.read_collection_len()?;
        let mut attributes = Vec::with_capacity(attribute_len);
        for _ in 0..attribute_len {
            attributes.push(read_attribute(reader)?);
        }
        let redaction_len = reader.read_collection_len()?;
        let mut redactions = Vec::with_capacity(redaction_len);
        for _ in 0..redaction_len {
            redactions.push(read_redaction(reader)?);
        }
        Self::new(
            event_id,
            trace_id,
            span_id,
            span_sequence,
            parent_span_id,
            causal_events,
            binding,
            time,
            kind,
            attributes,
            redactions,
        )
        .map_err(|_| invalid_domain(reader.offset()))
    }
}

fn write_binding(writer: &mut CanonicalWriter, value: CausalBinding) -> Result<(), CodecError> {
    writer.write_fixed(value.session_id().as_bytes())?;
    write_option_id(writer, value.run_id().map(RunId::into_bytes))?;
    write_option_id(writer, value.attempt_id().map(AttemptId::into_bytes))?;
    write_option_id(writer, value.turn_id().map(TurnId::into_bytes))?;
    write_option_id(writer, value.action_id().map(ActionId::into_bytes))?;
    write_option_id(writer, value.provider_profile_id().map(ProviderProfileId::into_bytes))?;
    write_option_digest(writer, value.tool_descriptor_digest())?;
    write_option_id(writer, value.gate_id().map(GateId::into_bytes))?;
    write_option_id(writer, value.gate_execution_id().map(GateExecutionId::into_bytes))
}

fn read_binding(reader: &mut CanonicalReader<'_>) -> Result<CausalBinding, CodecError> {
    let session_id = read_domain_id(reader, SessionId::new)?;
    let run_id = read_option_domain_id(reader, RunId::new)?;
    let attempt_id = read_option_domain_id(reader, AttemptId::new)?;
    let turn_id = read_option_domain_id(reader, TurnId::new)?;
    let action_id = read_option_domain_id(reader, ActionId::new)?;
    let provider_profile_id = read_option_domain_id(reader, ProviderProfileId::new)?;
    let tool_descriptor_digest = read_option_digest(reader)?;
    let gate_id = read_option_domain_id(reader, GateId::new)?;
    let gate_execution_id = read_option_domain_id(reader, GateExecutionId::new)?;
    CausalBinding::from_parts(
        session_id,
        run_id,
        attempt_id,
        turn_id,
        action_id,
        provider_profile_id,
        tool_descriptor_digest,
        gate_id,
        gate_execution_id,
    )
    .map_err(|_| invalid_domain(reader.offset()))
}

fn write_kind(writer: &mut CanonicalWriter, value: ObservationKind) -> Result<(), CodecError> {
    match value {
        ObservationKind::SpanStarted(kind) => {
            writer.write_u8(1)?;
            writer.write_u8(kind.tag())
        }
        ObservationKind::Diagnostic(code) => {
            writer.write_u8(2)?;
            writer.write_u16(code.tag())
        }
        ObservationKind::SpanEnded(outcome) => {
            writer.write_u8(3)?;
            writer.write_u8(outcome.tag())
        }
    }
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<ObservationKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => read_tag(reader, SpanKind::from_tag).map(ObservationKind::SpanStarted),
        2 => read_u16_tag(reader, DiagnosticCode::from_tag).map(ObservationKind::Diagnostic),
        3 => read_tag(reader, SpanOutcome::from_tag).map(ObservationKind::SpanEnded),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn write_attribute(writer: &mut CanonicalWriter, value: SafeAttribute) -> Result<(), CodecError> {
    writer.write_u16(value.key().tag())?;
    match value.value() {
        SafeAttributeValue::Count(count) => {
            writer.write_u8(1)?;
            writer.write_u64(count)
        }
        SafeAttributeValue::DurationNanos(nanos) => {
            writer.write_u8(2)?;
            writer.write_u64(nanos)
        }
        SafeAttributeValue::Identifier(identifier) => {
            writer.write_u8(3)?;
            writer.write_fixed(&identifier)
        }
        SafeAttributeValue::Digest(digest) => {
            writer.write_u8(4)?;
            writer.write_fixed(digest.as_bytes())
        }
        SafeAttributeValue::Status(status) => {
            writer.write_u8(5)?;
            writer.write_u8(status.tag())
        }
        SafeAttributeValue::Vault(reference) => {
            writer.write_u8(6)?;
            write_vault(writer, reference)
        }
    }
}

fn read_attribute(reader: &mut CanonicalReader<'_>) -> Result<SafeAttribute, CodecError> {
    let key = read_u16_tag(reader, SafeAttributeKey::from_tag)?;
    let offset = reader.offset();
    let value = match reader.read_u8()? {
        1 => SafeAttributeValue::Count(reader.read_u64()?),
        2 => SafeAttributeValue::DurationNanos(reader.read_u64()?),
        3 => SafeAttributeValue::Identifier(reader.read_fixed::<16>()?),
        4 => SafeAttributeValue::Digest(Sha256Digest::new(reader.read_fixed::<32>()?)),
        5 => SafeAttributeValue::Status(read_tag(reader, StatusCode::from_tag)?),
        6 => SafeAttributeValue::Vault(read_vault(reader)?),
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    };
    Ok(SafeAttribute::new(key, value))
}

fn write_redaction(writer: &mut CanonicalWriter, value: RedactedValue) -> Result<(), CodecError> {
    match value {
        RedactedValue::Omitted { class, observed_bytes } => {
            writer.write_u8(1)?;
            writer.write_u8(class.tag())?;
            writer.write_u64(observed_bytes)
        }
        RedactedValue::Vault { class, reference } => {
            writer.write_u8(2)?;
            writer.write_u8(class.tag())?;
            write_vault(writer, reference)
        }
    }
}

fn read_redaction(reader: &mut CanonicalReader<'_>) -> Result<RedactedValue, CodecError> {
    let offset = reader.offset();
    let disposition = reader.read_u8()?;
    let class = read_tag(reader, SensitivityClass::from_tag)?;
    match disposition {
        1 => Ok(RedactedValue::omitted(class, reader.read_u64()?)),
        2 => Ok(RedactedValue::Vault { class, reference: read_vault(reader)? }),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn write_vault(
    writer: &mut CanonicalWriter,
    value: ArtifactVaultReference,
) -> Result<(), CodecError> {
    writer.write_fixed(value.digest().as_bytes())?;
    writer.write_u64(value.size())?;
    writer.write_fixed(value.creating_event().as_bytes())?;
    writer.write_fixed(value.key_reference().as_bytes())?;
    writer.write_fixed(value.parameters_digest().as_bytes())
}

fn read_vault(reader: &mut CanonicalReader<'_>) -> Result<ArtifactVaultReference, CodecError> {
    let digest = ArtifactDigest::new(reader.read_fixed::<32>()?);
    let size = reader.read_u64()?;
    let creating_event = read_domain_id(reader, EventId::new)?;
    let key_reference = Sha256Digest::new(reader.read_fixed::<32>()?);
    let parameters_digest = Sha256Digest::new(reader.read_fixed::<32>()?);
    Ok(ArtifactVaultReference::from_parts(
        digest,
        size,
        creating_event,
        key_reference,
        parameters_digest,
    ))
}

fn write_option_span(
    writer: &mut CanonicalWriter,
    value: Option<SpanId>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        writer.write_fixed(value.as_bytes())?;
    }
    Ok(())
}

fn read_option_span(reader: &mut CanonicalReader<'_>) -> Result<Option<SpanId>, CodecError> {
    if reader.read_option_tag()? { read_span_id(reader).map(Some) } else { Ok(None) }
}

fn write_option_id(
    writer: &mut CanonicalWriter,
    value: Option<[u8; 16]>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        writer.write_fixed(&value)?;
    }
    Ok(())
}

fn write_option_digest(
    writer: &mut CanonicalWriter,
    value: Option<Sha256Digest>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        writer.write_fixed(value.as_bytes())?;
    }
    Ok(())
}

fn read_option_digest(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<Sha256Digest>, CodecError> {
    if reader.read_option_tag()? {
        Ok(Some(Sha256Digest::new(reader.read_fixed::<32>()?)))
    } else {
        Ok(None)
    }
}

fn read_trace_id(reader: &mut CanonicalReader<'_>) -> Result<TraceId, CodecError> {
    let offset = reader.offset();
    TraceId::new(reader.read_fixed::<16>()?).map_err(|_| invalid_domain(offset))
}

fn read_span_id(reader: &mut CanonicalReader<'_>) -> Result<SpanId, CodecError> {
    let offset = reader.offset();
    SpanId::new(reader.read_fixed::<8>()?).map_err(|_| invalid_domain(offset))
}

fn read_domain_id<T, E>(
    reader: &mut CanonicalReader<'_>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    constructor(reader.read_fixed::<16>()?).map_err(|_| invalid_domain(offset))
}

fn read_option_domain_id<T, E>(
    reader: &mut CanonicalReader<'_>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<Option<T>, CodecError> {
    if reader.read_option_tag()? { read_domain_id(reader, constructor).map(Some) } else { Ok(None) }
}

fn read_tag<T>(
    reader: &mut CanonicalReader<'_>,
    from_tag: impl FnOnce(u8) -> Option<T>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    from_tag(reader.read_u8()?).ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))
}

fn read_u16_tag<T>(
    reader: &mut CanonicalReader<'_>,
    from_tag: impl FnOnce(u16) -> Option<T>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    from_tag(reader.read_u16()?).ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))
}

const fn invalid_domain(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, offset)
}
