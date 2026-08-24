//! Direct protocol/router observation projections.

use peritus_conformance::{
    ToolConformanceObservation, ToolDescriptorObservation, ToolDisposition, ToolEffectObservation,
    ToolReplayObservation, ToolResultObservation,
};
use peritus_tool_protocol::{ResultStatus, Retryability, ToolResult};

pub fn with_result(
    disposition: ToolDisposition,
    schema_accepted: bool,
    effects: ToolEffectObservation,
    result: &ToolResult,
) -> ToolConformanceObservation {
    ToolConformanceObservation::new(
        disposition,
        None,
        schema_accepted,
        false,
        true,
        effects,
        result_observation(result),
        Vec::new(),
        false,
        true,
        ToolReplayObservation::default(),
    )
}

pub fn observed(
    disposition: ToolDisposition,
    descriptor: Option<ToolDescriptorObservation>,
    schema_accepted: bool,
    exposed: bool,
    canonical: bool,
) -> ToolConformanceObservation {
    ToolConformanceObservation::new(
        disposition,
        descriptor,
        schema_accepted,
        exposed,
        canonical,
        ToolEffectObservation::default(),
        empty_result(),
        Vec::new(),
        false,
        true,
        ToolReplayObservation::default(),
    )
}

pub fn observed_with_effects(
    disposition: ToolDisposition,
    schema_accepted: bool,
    effects: ToolEffectObservation,
) -> ToolConformanceObservation {
    ToolConformanceObservation::new(
        disposition,
        None,
        schema_accepted,
        false,
        true,
        effects,
        empty_result(),
        Vec::new(),
        false,
        true,
        ToolReplayObservation::default(),
    )
}

pub fn result_observation(result: &ToolResult) -> ToolResultObservation {
    ToolResultObservation::new(
        result.structured().is_some(),
        result.failure_value().is_some(),
        result.human_rendering().as_str().len() as u64,
        result.model_rendering().as_str().len() as u64,
        result.artifacts().len() as u64,
        result.failure_value().is_some_and(|failure| failure.retryability() != Retryability::Never),
        true,
        false,
    )
}

pub const fn empty_result() -> ToolResultObservation {
    ToolResultObservation::new(false, false, 0, 0, 0, false, false, false)
}

pub const fn disposition(status: ResultStatus) -> ToolDisposition {
    match status {
        ResultStatus::Succeeded => ToolDisposition::Succeeded,
        ResultStatus::Failed => ToolDisposition::Failed,
        ResultStatus::Cancelled => ToolDisposition::Cancelled,
        ResultStatus::TimedOut => ToolDisposition::TimedOut,
        ResultStatus::Indeterminate => ToolDisposition::Indeterminate,
    }
}
