//! Provider-facing function tools, completed calls/results, and output/reasoning policy.

use crate::{
    BoundedText, CanonicalJson, JsonSchema, OutputName, ProtocolError, ProtocolErrorKind,
    ProtocolLimits, ToolCallId, ToolName,
};

/// Portable application function declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolDefinition {
    name: ToolName,
    description: Option<BoundedText>,
    parameters: JsonSchema,
    strict: bool,
}

impl ToolDefinition {
    /// Creates a function declaration.
    #[must_use]
    pub const fn new(
        name: ToolName,
        description: Option<BoundedText>,
        parameters: JsonSchema,
        strict: bool,
    ) -> Self {
        Self { name, description, parameters, strict }
    }

    /// Borrows the function name.
    #[must_use]
    pub const fn name(&self) -> &ToolName {
        &self.name
    }
    /// Borrows the optional sensitive description.
    #[must_use]
    pub const fn description(&self) -> Option<&BoundedText> {
        self.description.as_ref()
    }
    /// Borrows the parameters schema.
    #[must_use]
    pub const fn parameters(&self) -> &JsonSchema {
        &self.parameters
    }
    /// Returns strict tool-input validation policy.
    #[must_use]
    pub const fn strict(&self) -> bool {
        self.strict
    }
}

/// Provider function-selection policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolChoice {
    /// Provider selects whether to call a function.
    Auto,
    /// Functions are disabled for this request.
    None,
    /// At least one function call is required.
    Required,
    /// One exact function is required.
    Specific(ToolName),
}

/// Parallel function-call behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelToolPolicy {
    /// Calls must be serialized.
    Disabled,
    /// Provider may return at most this many simultaneous calls.
    Allowed(u32),
}

/// A tool call is constructible only after its complete arguments parse as a JSON object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedToolCall {
    id: ToolCallId,
    name: ToolName,
    arguments: CanonicalJson,
}

impl CompletedToolCall {
    /// Creates one completed call.
    ///
    /// # Errors
    ///
    /// Rejects non-object function arguments.
    pub fn new(
        id: ToolCallId,
        name: ToolName,
        arguments: CanonicalJson,
    ) -> Result<Self, ProtocolError> {
        if !arguments.is_object() {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "tool_call.arguments",
                "completed function arguments must be a JSON object",
            ));
        }
        Ok(Self { id, name, arguments })
    }

    /// Borrows the provider call identity.
    #[must_use]
    pub const fn id(&self) -> &ToolCallId {
        &self.id
    }
    /// Borrows the declared tool name.
    #[must_use]
    pub const fn name(&self) -> &ToolName {
        &self.name
    }
    /// Borrows complete canonical arguments.
    #[must_use]
    pub const fn arguments(&self) -> &CanonicalJson {
        &self.arguments
    }
}

/// Application result correlated to one provider call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolResult {
    call_id: ToolCallId,
    output: CanonicalJson,
    is_error: bool,
}

impl ToolResult {
    /// Creates a bounded result observation; it grants no execution authority.
    #[must_use]
    pub const fn new(call_id: ToolCallId, output: CanonicalJson, is_error: bool) -> Self {
        Self { call_id, output, is_error }
    }

    /// Borrows the correlated call identity.
    #[must_use]
    pub const fn call_id(&self) -> &ToolCallId {
        &self.call_id
    }
    /// Borrows the canonical result.
    #[must_use]
    pub const fn output(&self) -> &CanonicalJson {
        &self.output
    }
    /// Returns whether the application tool reported failure.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        self.is_error
    }
}

/// Requested final output contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StructuredOutput {
    /// Ordinary provider text.
    Text,
    /// Legacy valid-JSON object mode without a schema.
    JsonObject,
    /// JSON constrained by a named schema.
    JsonSchema {
        /// Provider-visible output contract name.
        name: OutputName,
        /// Bounded schema document.
        schema: JsonSchema,
        /// Whether schema compliance is required on normal completion.
        strict: bool,
    },
}

/// Provider-neutral reasoning effort level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningEffort {
    /// Minimum supported reasoning effort.
    Minimal,
    /// Low reasoning effort.
    Low,
    /// Medium reasoning effort.
    Medium,
    /// High reasoning effort.
    High,
}

/// Visible reasoning-summary request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SummaryPolicy {
    /// No visible summary.
    None,
    /// Provider chooses the supported summary form.
    Auto,
    /// Concise visible summary.
    Concise,
    /// Detailed visible summary.
    Detailed,
}

/// Requested reasoning behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningPolicy {
    /// No model-specific reasoning control.
    Disabled,
    /// Provider/model chooses effort.
    Adaptive {
        /// Requested visible-summary behavior.
        summary: SummaryPolicy,
    },
    /// Caller selects one supported effort.
    Effort {
        /// Requested portable effort level.
        effort: ReasoningEffort,
        /// Requested visible-summary behavior.
        summary: SummaryPolicy,
    },
}

impl ReasoningPolicy {
    pub(crate) const fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    pub(crate) const fn requests_summary(self) -> bool {
        match self {
            Self::Disabled => false,
            Self::Adaptive { summary } | Self::Effort { summary, .. } => {
                !matches!(summary, SummaryPolicy::None)
            }
        }
    }
}

/// Validates parallel-call policy against protocol and negotiated model limits.
pub(crate) fn validate_parallel(
    policy: ParallelToolPolicy,
    model_max: u32,
    limits: ProtocolLimits,
) -> Result<(), ProtocolError> {
    if let ParallelToolPolicy::Allowed(count) = policy
        && (count == 0
            || count > model_max
            || usize::try_from(count).map_or(true, |value| value > limits.max_tools()))
    {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidRequest,
            "parallel_tool_policy",
            "parallel tool count is zero or exceeds negotiated limits",
        ));
    }
    Ok(())
}
