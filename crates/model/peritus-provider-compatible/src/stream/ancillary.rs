use peritus_model_protocol::{
    CanonicalJson, ExtensionName, JsonBounds, ModelEvent, ProtocolLimits, ProviderExtension,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use crate::error;

pub(super) fn event(
    value: &Value,
    limits: ProtocolLimits,
) -> Result<ModelEvent, ProviderCoreError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| error::malformed("compatible ancillary serialization failed"))?;
    let text = core::str::from_utf8(&bytes)
        .map_err(|_| error::malformed("compatible ancillary value was not UTF-8"))?;
    let canonical = CanonicalJson::parse(text, JsonBounds::value(limits))
        .map_err(|_| error::malformed("compatible ancillary event exceeded bounds"))?;
    let name = ExtensionName::new("compatible.ancillary".to_owned())
        .map_err(|_| error::malformed("static compatible extension name was invalid"))?;
    Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, canonical)))
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
