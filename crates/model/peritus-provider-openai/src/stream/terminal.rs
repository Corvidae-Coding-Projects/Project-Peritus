//! Response lifecycle, usage/cache, and provider terminal normalization.

use peritus_model_protocol::{
    CacheObservation, CacheStatus, FailureCategory, FinishReason, ItemKind, ModelEvent, ModelName,
    OutcomeCertainty, ResponseId, Retryability, TransportPhase, UsageCounters, UsageObservation,
    UsageScope,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::OpenAiStream;
use crate::error;

impl OpenAiStream {
    pub(super) fn response_created(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let response = response_object(value)?;
        let identity = response_id(response)?;
        let model_text = string_field(response, "model")?;
        if model_text != self.expected_model.as_str()
            || !matches!(string_field(response, "status")?, "queued" | "in_progress")
            || !self.state.start(identity.clone())
        {
            return Err(error::malformed("OpenAI response creation contradicted the request"));
        }
        if self.register_background {
            let mut registry = self
                .resumable
                .lock()
                .map_err(|_| error::malformed("OpenAI continuation registry was unavailable"))?;
            if registry.len() < 4_096 {
                registry.insert(identity.clone());
            }
        }
        let model = ModelName::new(model_text.to_owned())
            .map_err(|_| error::malformed("OpenAI response model identity is invalid"))?;
        let mut events =
            vec![ModelEvent::ResponseStarted { response_id: Some(identity), model: Some(model) }];
        if let Some(request_id) = self.metadata.take_request_id() {
            events.push(provider_event("openai.request_id", &request_id)?);
        }
        if let Some(rate_limit) = self.metadata.take_rate_limit() {
            events.push(ModelEvent::RateLimit(rate_limit));
        }
        Ok(events)
    }

    pub(super) fn response_completed(
        &self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let response = response_object(value)?;
        validate_terminal_response(self, response, "completed")?;
        if !self.state.all_items_complete() {
            return Err(error::malformed("OpenAI completed before every output item closed"));
        }
        let mut events = Vec::new();
        if let Some((usage, cached)) = usage(response)? {
            events.push(ModelEvent::Usage(usage));
            if let Some(cached_tokens) = cached.filter(|tokens| *tokens > 0) {
                events.push(ModelEvent::Cache(CacheObservation::new(
                    CacheStatus::Hit,
                    None,
                    Some(cached_tokens),
                    None,
                )));
            }
        }
        let finish = if self.state.has_kind(ItemKind::Refusal) {
            FinishReason::Refusal
        } else if self.state.has_kind(ItemKind::ToolCall) {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        };
        events.push(ModelEvent::Finish(finish));
        events.push(ModelEvent::ResponseCompleted);
        Ok(events)
    }

    pub(super) fn response_failed(
        &self,
        value: &Value,
        incomplete: bool,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let response = response_object(value)?;
        validate_terminal_response(
            self,
            response,
            if incomplete { "incomplete" } else { "failed" },
        )?;
        let response_id = self.state.response_id().cloned();
        let (category, code) = if incomplete {
            (FailureCategory::IncompleteStream, "openai.response.incomplete")
        } else {
            (FailureCategory::Provider, "openai.response.failed")
        };
        let failure = error::failure(
            &self.provider,
            category,
            TransportPhase::Completed,
            OutcomeCertainty::Terminal,
            Retryability::Never,
            Some(200),
            response_id,
            None,
            code,
        )?;
        let mut events = Vec::new();
        if incomplete {
            events.push(ModelEvent::Finish(incomplete_reason(response)));
        }
        events.push(ModelEvent::ResponseFailed(failure));
        Ok(events)
    }

    pub(super) fn stream_error(
        &self,
        _value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let failure = error::failure(
            &self.provider,
            FailureCategory::Provider,
            TransportPhase::StreamObserved,
            OutcomeCertainty::AcceptedPartial,
            Retryability::Never,
            Some(200),
            self.state.response_id().cloned(),
            None,
            "openai.stream.error",
        )?;
        Ok(vec![ModelEvent::ResponseFailed(failure)])
    }

    pub(super) fn ancillary(value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let serialized = serde_json::to_string(value)
            .map_err(|_| error::malformed("OpenAI ancillary event serialization failed"))?;
        let value = peritus_model_protocol::CanonicalJson::parse(
            &serialized,
            peritus_model_protocol::JsonBounds::value(
                peritus_model_protocol::ProtocolLimits::PRODUCTION,
            ),
        )
        .map_err(|_| error::malformed("OpenAI ancillary event exceeded protocol bounds"))?;
        let name = peritus_model_protocol::ExtensionName::new("openai.ancillary".to_owned())
            .map_err(|_| error::malformed("static OpenAI extension identity was invalid"))?;
        Ok(vec![ModelEvent::ProviderEvent(peritus_model_protocol::ProviderExtension::new(
            name, value,
        ))])
    }
}

fn validate_terminal_response(
    stream: &OpenAiStream,
    response: &Value,
    status: &str,
) -> Result<(), ProviderCoreError> {
    let identity = string_field(response, "id")?;
    if !stream.state.started()
        || !stream.state.response_matches(identity)
        || string_field(response, "status")? != status
    {
        return Err(error::malformed("OpenAI response terminal contradicted lifecycle state"));
    }
    Ok(())
}

fn usage(response: &Value) -> Result<Option<(UsageObservation, Option<u64>)>, ProviderCoreError> {
    let Some(usage) = response.get("usage") else { return Ok(None) };
    if usage.is_null() {
        return Ok(None);
    }
    let input = optional_u64(usage, "input_tokens")?;
    let output = optional_u64(usage, "output_tokens")?;
    let total = optional_u64(usage, "total_tokens")?;
    let cached = usage
        .get("input_tokens_details")
        .filter(|value| !value.is_null())
        .map(|details| optional_u64(details, "cached_tokens"))
        .transpose()?
        .flatten();
    let reasoning = usage
        .get("output_tokens_details")
        .filter(|value| !value.is_null())
        .map(|details| optional_u64(details, "reasoning_tokens"))
        .transpose()?
        .flatten();
    if matches!((input, output, total), (Some(input), Some(output), Some(total)) if input.checked_add(output) != Some(total))
    {
        return Err(error::malformed("OpenAI usage total contradicted input and output"));
    }
    Ok(Some((
        UsageObservation::new(
            UsageScope::Final,
            UsageCounters::new(input, cached, None, output, reasoning, None, total, None),
            None,
        ),
        cached,
    )))
}

fn incomplete_reason(response: &Value) -> FinishReason {
    match response
        .get("incomplete_details")
        .and_then(|value| value.get("reason"))
        .and_then(Value::as_str)
    {
        Some("max_output_tokens") => FinishReason::Length,
        Some("content_filter") => FinishReason::Safety,
        _ => FinishReason::Incomplete,
    }
}

fn provider_event(name: &str, value: &str) -> Result<ModelEvent, ProviderCoreError> {
    let name = peritus_model_protocol::ExtensionName::new(name.to_owned())
        .map_err(|_| error::malformed("static OpenAI extension identity was invalid"))?;
    let encoded = serde_json::to_string(value)
        .map_err(|_| error::malformed("OpenAI provider observation serialization failed"))?;
    let value = peritus_model_protocol::CanonicalJson::parse(
        &encoded,
        peritus_model_protocol::JsonBounds::value(
            peritus_model_protocol::ProtocolLimits::PRODUCTION,
        ),
    )
    .map_err(|_| error::malformed("OpenAI provider observation exceeded bounds"))?;
    Ok(ModelEvent::ProviderEvent(peritus_model_protocol::ProviderExtension::new(name, value)))
}

fn response_object(value: &Value) -> Result<&Value, ProviderCoreError> {
    value
        .get("response")
        .filter(|response| response.is_object())
        .ok_or_else(|| error::malformed("OpenAI lifecycle event omitted its response"))
}

fn response_id(value: &Value) -> Result<ResponseId, ProviderCoreError> {
    ResponseId::new(string_field(value, "id")?.to_owned())
        .map_err(|_| error::malformed("OpenAI response identity is invalid"))
}

fn string_field<'a>(value: &'a Value, name: &str) -> Result<&'a str, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error::malformed("OpenAI response omitted a required string"))
}

fn optional_u64(value: &Value, name: &str) -> Result<Option<u64>, ProviderCoreError> {
    match value.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| error::malformed("OpenAI usage counter is not a nonnegative integer")),
    }
}
