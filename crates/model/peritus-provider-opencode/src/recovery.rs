//! Retry only a completed, inert model turn; never repair native transport or host effects.

use peritus_model_protocol::{
    CanonicalJson, ExtensionName, JsonBounds, ModelEvent, ProtocolLimits, ProviderExtension,
    UsageCounters,
};
use peritus_provider_core::ProviderCoreError;

#[derive(Clone, Copy)]
pub(super) enum TurnRejection {
    Json,
    Envelope,
    Content,
    Tools,
    Arguments,
}

impl TurnRejection {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Json => "opencode.runtime.invalid_turn_json",
            Self::Envelope => "opencode.runtime.invalid_turn_envelope",
            Self::Content => "opencode.runtime.invalid_turn_content",
            Self::Tools => "opencode.runtime.invalid_turn_tools",
            Self::Arguments => "opencode.runtime.invalid_tool_arguments",
        }
    }

    const fn explanation(self) -> &'static str {
        match self {
            Self::Json => "the reply was not one complete valid JSON object",
            Self::Envelope => "the object must have exactly content and tool_calls",
            Self::Content => {
                "content must be a string or report object; an empty string needs tool calls"
            }
            Self::Tools => "tool_calls must obey the declared names, selection, and maximum count",
            Self::Arguments => "each call must contain exactly name and object-valued arguments",
        }
    }

    pub(super) fn correction(self, original: &[u8]) -> Vec<u8> {
        let mut prompt = original.to_vec();
        prompt.extend_from_slice(format!(
            "\n\nPERITUS TRANSPORT CORRECTION\nThe previous inference completed but {}. None of its proposed tools were executed. Regenerate the same next turn from the original conversation above. Return exactly one JSON object with content and tool_calls, followed by no other text. Escape quotes, newlines and backslashes within JSON strings. Keep both fields inside the same outer object. Empty content is permitted only when requesting tools. Do not simulate tool results or restart completed observations.\n",
            self.explanation(),
        ).as_bytes());
        prompt
    }

    pub(super) fn audit(self, attempt: u32) -> Result<ModelEvent, ProviderCoreError> {
        // No provider text, credentials, or invented public assistant message enters diagnostics.
        let value =
            serde_json::json!({"attempt":attempt,"reason":self.code(),"tools_executed":false});
        let value =
            CanonicalJson::parse(&value.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))
                .map_err(|_| super::malformed())?;
        let name = ExtensionName::new("peritus.opencode_turn_correction".to_owned())
            .map_err(|_| super::malformed())?;
        Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)))
    }
}

pub(super) fn combined_usage(
    previous: UsageCounters,
    current: UsageCounters,
) -> Result<UsageCounters, ProviderCoreError> {
    let add = |a: Option<u64>, b: Option<u64>| match (a, b) {
        (Some(a), Some(b)) => a.checked_add(b).map(Some).ok_or_else(super::malformed),
        // Missing vendor accounting is unknown, never a synthesized zero.
        _ => Ok(None),
    };
    Ok(UsageCounters::new(
        add(previous.input_tokens(), current.input_tokens())?,
        add(previous.cached_input_tokens(), current.cached_input_tokens())?,
        add(previous.cache_creation_input_tokens(), current.cache_creation_input_tokens())?,
        add(previous.output_tokens(), current.output_tokens())?,
        add(previous.reasoning_output_tokens(), current.reasoning_output_tokens())?,
        add(previous.tool_tokens(), current.tool_tokens())?,
        add(previous.total_tokens(), current.total_tokens())?,
        add(previous.provider_cost_microunits(), current.provider_cost_microunits())?,
    ))
}
