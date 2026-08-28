//! Strict independent-review result parsing.

use serde::Deserialize;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewResult {
    pub summary: String,
    pub blocking: bool,
    #[serde(default)]
    pub findings: Vec<String>,
}

pub fn parse(value: &str) -> Result<ReviewResult, ProductRunnerError> {
    let start = value.find('{').ok_or_else(|| invalid("review contains no JSON object"))?;
    let end = value.rfind('}').ok_or_else(|| invalid("review contains no complete JSON object"))?;
    let review: ReviewResult = serde_json::from_str(&value[start..=end]).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "parse reviewer result",
            error.to_string(),
        )
    })?;
    if review.summary.trim().is_empty() || review.findings.len() > 128 {
        return Err(invalid("review summary is empty or has too many findings"));
    }
    Ok(review)
}

fn invalid(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate reviewer result",
        detail,
    )
}
