//! Responses lifecycle terminals and final usage normalization.

use peritus_model_protocol::{
    FailureCategory, FinishReason, ItemKind, ModelEvent, OutcomeCertainty, Retryability,
    TransportPhase, UsageCounters, UsageObservation, UsageScope,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::{ResponsesDecoder, object, string};
use crate::error;

impl ResponsesDecoder {
    pub(super) fn completed(&self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let response = self.terminal_response(value, "completed")?;
        if !self.state.all_complete() {
            return Err(error::malformed("Responses-compatible terminal preceded item closure"));
        }
        let mut events = usage(response, self.allow_usage)?;
        let reason = if self.state.has_kind(ItemKind::Refusal) {
            FinishReason::Refusal
        } else if self.state.has_kind(ItemKind::ToolCall) {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        };
        events.push(ModelEvent::Finish(reason));
        events.push(ModelEvent::ResponseCompleted);
        Ok(events)
    }

    pub(super) fn failed(
        &self,
        value: &Value,
        incomplete: bool,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let response =
            self.terminal_response(value, if incomplete { "incomplete" } else { "failed" })?;
        let failure = error::failure(
            &self.provider,
            if incomplete { FailureCategory::IncompleteStream } else { FailureCategory::Provider },
            TransportPhase::Completed,
            OutcomeCertainty::Terminal,
            Retryability::Never,
            Some(200),
            self.state.response_id().cloned(),
            None,
            if incomplete {
                "compatible.response.incomplete"
            } else {
                "compatible.response.failed"
            },
        )?;
        let mut events = usage(response, self.allow_usage)?;
        if incomplete {
            events.push(ModelEvent::Finish(FinishReason::Incomplete));
        }
        events.push(ModelEvent::ResponseFailed(failure));
        Ok(events)
    }

    pub(super) fn stream_error(&self) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let failure = error::failure(
            &self.provider,
            FailureCategory::Provider,
            TransportPhase::StreamObserved,
            OutcomeCertainty::AcceptedPartial,
            Retryability::Never,
            Some(200),
            self.state.response_id().cloned(),
            None,
            "compatible.stream.error",
        )?;
        Ok(vec![ModelEvent::ResponseFailed(failure)])
    }

    fn terminal_response<'a>(
        &self,
        value: &'a Value,
        status: &str,
    ) -> Result<&'a Value, ProviderCoreError> {
        let response = object(value, "response")?;
        if !self.state.started()
            || !self.state.response_matches(string(response, "id")?)
            || string(response, "status")? != status
            || string(response, "model")? != self.model.as_str()
        {
            return Err(error::malformed("Responses-compatible terminal contradicted lifecycle"));
        }
        Ok(response)
    }
}

fn usage(response: &Value, allowed: bool) -> Result<Vec<ModelEvent>, ProviderCoreError> {
    let Some(value) = response.get("usage") else { return Ok(Vec::new()) };
    if value.is_null() {
        return Ok(Vec::new());
    }
    if !allowed {
        return Err(error::malformed("Responses-compatible usage was not declared by the profile"));
    }
    let input = optional_integer(value, "input_tokens")?;
    let output = optional_integer(value, "output_tokens")?;
    let total = optional_integer(value, "total_tokens")?;
    if matches!((input, output, total), (Some(a), Some(b), Some(c)) if a.checked_add(b) != Some(c))
    {
        return Err(error::malformed("compatible usage total was inconsistent"));
    }
    Ok(vec![ModelEvent::Usage(UsageObservation::new(
        UsageScope::Final,
        UsageCounters::new(input, None, None, output, None, None, total, None),
        None,
    ))])
}

fn optional_integer(value: &Value, name: &str) -> Result<Option<u64>, ProviderCoreError> {
    match value.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            error::malformed("compatible usage counter was not a nonnegative integer")
        }),
    }
}
