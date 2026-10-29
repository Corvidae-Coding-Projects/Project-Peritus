//! Run-owned accounting is updated before another provider or tool boundary can fail.

use super::FileDeveloperTrace;
use crate::budget::RunAccounting;
use peritus_agent::{
    DeveloperAccountingEvent, DeveloperLoopError, DeveloperRequestMetrics, DeveloperTrace,
    DeveloperTraceEvent,
};
use std::path::Path;

fn usage_json(usage: peritus_model_protocol::UsageCounters) -> serde_json::Value {
    serde_json::json!({"input_tokens":usage.input_tokens(),"cached_input_tokens":usage.cached_input_tokens(),
        "cache_creation_input_tokens":usage.cache_creation_input_tokens(),"output_tokens":usage.output_tokens(),
        "reasoning_output_tokens":usage.reasoning_output_tokens(),"tool_tokens":usage.tool_tokens(),
        "total_tokens":usage.total_tokens(),"provider_cost_microunits":usage.provider_cost_microunits()})
}

pub struct AccountingTrace<'a> {
    pub(crate) trace: FileDeveloperTrace,
    accounting: &'a mut RunAccounting,
    path: std::path::PathBuf,
    invocation: String,
    previous_projection: Option<peritus_provider_core::RequestProjection>,
    role: peritus_agent::DeveloperModelRole,
}

impl<'a> AccountingTrace<'a> {
    pub(crate) fn new(
        path: &Path,
        accounting: &'a mut RunAccounting,
        invocation: &str,
        role: peritus_agent::DeveloperModelRole,
    ) -> Self {
        Self {
            trace: FileDeveloperTrace::new(path.to_owned()),
            accounting,
            path: path.to_owned(),
            invocation: invocation.to_owned(),
            previous_projection: None,
            role,
        }
    }
}

impl DeveloperTrace for AccountingTrace<'_> {
    fn performance(&mut self, metrics: &DeveloperRequestMetrics) {
        if !peritus_provider_core::efficiency_diagnostics_enabled() {
            return;
        }
        let projections = metrics.projections.iter().map(|projection| {
            let measured = serde_json::json!({
                "layout": projection.layout(),
                "part_bytes": projection.part_bytes(),
                "part_characters": projection.part_characters(),
                "reason": projection.reason(),
                "components": projection.components().map(|parts| parts.iter().map(|part| serde_json::json!({
                    "kind":part.kind().name(),"bytes":part.bytes(),"characters":part.characters()})).collect::<Vec<_>>()),
                "component_semantics":"disjoint encoded spans; role/block and fixed host-marker attribution; mixed bodies remain mixed; remainder includes framing and encoding",
                "native_outcome": projection.native_outcome(),
                "native_response_usage": projection.native_usage().map(usage_json),
                "native_usage_semantics":"per native inference, where observed; do not add to combined logical request accounting; missing native usage is unknown",
                "common_prefix_bytes_lower_bound": self.previous_projection.as_ref()
                    .map(|previous| projection.common_prefix_bytes(previous)),
                "semantics": "actual adapter-rendered parts before vendor-owned framing; no claim of submission, token count, or provider cache acceptance"
            });
            self.previous_projection = Some(projection.clone());
            measured
        }).collect::<Vec<_>>();
        if projections.is_empty() {
            self.previous_projection = None;
        }
        crate::efficiency::event(
            &self.path,
            serde_json::json!({
                "event": "provider-attempt-work", "invocation": self.invocation,
            "request_id": metrics.request_id, "projections": projections,
            "role": format!("{:?}",self.role).to_ascii_lowercase(),
            "reason":metrics.reason,"turn":metrics.turn,"attempt":metrics.attempt,
            "projection_measurement_available": !projections.is_empty(),
                "start_micros": metrics.start_micros,
                "first_event_micros": metrics.first_event_micros,
                "first_text_micros": metrics.first_text_micros,
                "elapsed_micros": metrics.elapsed_micros,
                "usage_observed": metrics.usage_observed,
                "terminal_observed": metrics.terminal_observed,
                "terminal_outcome": metrics.terminal.as_ref().map(|terminal| match terminal {
                    peritus_model_protocol::TerminalOutcome::Succeeded { .. } => "succeeded",
                    peritus_model_protocol::TerminalOutcome::RequiresAction { .. } => "requires_action",
                    peritus_model_protocol::TerminalOutcome::Refused { .. } => "refused",
                    peritus_model_protocol::TerminalOutcome::Incomplete { .. } => "incomplete",
                    peritus_model_protocol::TerminalOutcome::Cancelled => "cancelled",
                    peritus_model_protocol::TerminalOutcome::Failed(_) => "failed",
                }),
                "drive_failed": metrics.drive_failed,
                "semantics": "one record per admitted attempt; absent usage is unknown cost, not zero; normalized accounting remains separate"
            }),
        );
    }

    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        self.trace.record(event)
    }

    fn account(&mut self, event: DeveloperAccountingEvent) -> Result<(), DeveloperLoopError> {
        self.accounting.record_event(event).map_err(|_| DeveloperLoopError::LimitExceeded)?;
        if !peritus_provider_core::efficiency_diagnostics_enabled() {
            return Ok(());
        }
        if matches!(
            event,
            DeveloperAccountingEvent::Usage(_) | DeveloperAccountingEvent::ModelRequest { .. }
        ) {
            let progress = self.accounting.latest_snapshot();
            let response_usage = match event {
                DeveloperAccountingEvent::Usage(usage) => serde_json::json!({
                    "input_tokens":usage.input_tokens(),"cached_input_tokens":usage.cached_input_tokens(),
                    "output_tokens":usage.output_tokens(),
                    "cache_creation_input_tokens":usage.cache_creation_input_tokens(),
                    "reasoning_output_tokens":usage.reasoning_output_tokens(),
                    "tool_tokens":usage.tool_tokens(),
                    "total_tokens":usage.total_tokens(),
                    "provider_cost_microunits":usage.provider_cost_microunits(),
                }),
                _ => serde_json::Value::Null,
            };
            crate::efficiency::event(
                &self.path,
                serde_json::json!({"event":"provider-accounting","invocation":self.invocation,
                "response_usage":response_usage,"role":format!("{:?}",self.role).to_ascii_lowercase(),
                "model_requests":progress.model_requests(),"input_tokens":progress.input_tokens(),
                "cached_input_tokens":progress.cached_input_tokens(),"output_tokens":progress.output_tokens(),
                "semantics":"cumulative normalized snapshots; replace, do not sum snapshots"}),
            );
        }
        Ok(())
    }
}
