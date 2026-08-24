//! Checked quality-run selection input.

use crate::{QualityError, QualityErrorKind};
use peritus_tool_protocol::BoundedJson;

/// Exact stable gate name selected by `quality.run`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunInput {
    gate_name: String,
}

impl RunInput {
    /// Creates a bounded stable gate selection.
    ///
    /// # Errors
    /// Returns a typed failure for empty, control-containing, or oversized names.
    pub fn new(gate_name: impl Into<String>) -> Result<Self, QualityError> {
        let gate_name = gate_name.into();
        if gate_name.is_empty() || gate_name.len() > 128 || gate_name.chars().any(char::is_control)
        {
            return Err(QualityError::new(
                QualityErrorKind::InvalidInput,
                "quality gate name is empty, contains controls, or exceeds its bound",
            ));
        }
        Ok(Self { gate_name })
    }

    /// Decodes already schema-validated `quality.run` arguments defensively.
    ///
    /// # Errors
    /// Returns a typed failure when the required `gate` string is absent or malformed.
    pub fn from_arguments(arguments: &BoundedJson) -> Result<Self, QualityError> {
        let gate = arguments
            .property("gate")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| {
            QualityError::new(
                QualityErrorKind::InvalidInput,
                "required string property \"gate\" is absent or invalid",
            )
        })?;
        Self::new(gate)
    }

    /// Returns the selected stable gate name.
    #[must_use]
    pub fn gate_name(&self) -> &str {
        &self.gate_name
    }
}
