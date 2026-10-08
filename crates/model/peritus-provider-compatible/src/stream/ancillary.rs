use peritus_model_protocol::{
    CanonicalJson, ExtensionName, JsonBounds, ModelEvent, OptionalObservation,
    OptionalObservationKind, OptionalObservationStatus, ProtocolLimits, ProviderExtension,
};
use serde_json::Value;

pub(super) fn event(value: &Value, limits: ProtocolLimits) -> ModelEvent {
    let encoded = value.to_string();
    let Ok(canonical) = CanonicalJson::parse(&encoded, JsonBounds::extension(limits)) else {
        return diagnostic_bytes(
            encoded.as_bytes(),
            OptionalObservationKind::Ancillary,
            OptionalObservationStatus::ExceededBound,
        );
    };
    let Ok(name) = ExtensionName::new("compatible.ancillary".to_owned()) else {
        return diagnostic_bytes(
            encoded.as_bytes(),
            OptionalObservationKind::Ancillary,
            OptionalObservationStatus::InvalidValue,
        );
    };
    ModelEvent::ProviderEvent(ProviderExtension::new(name, canonical))
}

pub(super) fn diagnostic(
    value: &Value,
    kind: OptionalObservationKind,
    status: OptionalObservationStatus,
) -> ModelEvent {
    diagnostic_bytes(value.to_string().as_bytes(), kind, status)
}

fn diagnostic_bytes(
    value: &[u8],
    kind: OptionalObservationKind,
    status: OptionalObservationStatus,
) -> ModelEvent {
    ModelEvent::OptionalObservation(OptionalObservation::new(kind, status, value))
}

pub(super) fn safe_responses(event_type: &str) -> bool {
    event_type.starts_with("provider.")
        || (!event_type.starts_with("response.output_")
            && !event_type.starts_with("response.content_")
            && !event_type.starts_with("response.function_")
            && (event_type.ends_with(".queued")
                || event_type.ends_with(".searching")
                || event_type.ends_with(".in_progress")))
}
