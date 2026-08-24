//! Truthful bounded normalization of dispatcher failures.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedText, PreparedToolCall, ToolResult, ToolTiming, Truncation, TruncationMetadata,
};

use crate::{DispatchFailure, RouterError, RouterErrorKind};

pub fn normalize_failure(
    prepared: &PreparedToolCall,
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
    failure: &DispatchFailure,
    progress_count: u32,
) -> Result<ToolResult, RouterError> {
    let model = bounded_render(
        failure.failure().detail().as_str(),
        prepared.call().limits().model_bytes(),
    )?;
    let human = bounded_render(
        failure.failure().detail().as_str(),
        prepared.call().limits().human_bytes(),
    )?;
    let timing = ToolTiming::new(started_at, finished_at)
        .map_err(|_| invalid("dispatcher failure timing is invalid"))?;
    ToolResult::failure(
        prepared,
        failure.status(),
        failure.failure().clone(),
        None,
        human,
        model,
        Vec::new(),
        timing,
        TruncationMetadata {
            output: Truncation::Indeterminate,
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        progress_count,
    )
    .map_err(|_| invalid("dispatcher failure cannot be normalized into a bounded result"))
}

fn bounded_render(value: &str, maximum: u32) -> Result<BoundedText, RouterError> {
    let limit = maximum as usize;
    let end = if value.len() <= limit {
        value.len()
    } else {
        let mut boundary = limit;
        while boundary > 0 && !value.is_char_boundary(boundary) {
            boundary -= 1;
        }
        boundary
    };
    let rendered = if end == 0 { "!" } else { &value[..end] };
    BoundedText::new(rendered.to_owned()).map_err(|_| invalid("failure rendering is invalid"))
}

const fn invalid(detail: &'static str) -> RouterError {
    RouterError::new(RouterErrorKind::InvalidObservation, "accept tool observation", detail)
}
