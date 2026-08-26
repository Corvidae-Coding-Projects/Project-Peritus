//! Frozen C6-to-C5 request mapping and complete model plan identity.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_context::{ContextPlanId, MessageRole, RenderPlan};
use peritus_model_protocol::{
    BoundedText, CachePolicy, ContentBlock, JsonBounds, JsonSchema, Message, ModelRequest,
    ParallelToolPolicy, ProtocolLimits, ReasoningPolicy, Role, SchemaDialect, StructuredOutput,
    ToolChoice,
};
use peritus_types::Sha256Digest;

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerLimit, DebuggerLimits, DebuggerOperation,
    DebuggerRecovery, ModelAnalysisId, ModelBudget, ModelRetryPolicy, SelectionManifestId,
    TraceSelectionManifest,
};

const PLAN_DOMAIN: &[u8] = b"peritus.debugger.model-plan.v1\0";

/// Closed strict schema for the only accepted optional model proposal.
pub const MODEL_PROPOSAL_SCHEMA: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","additionalProperties":false,"properties":{"deterministic_digest":{"pattern":"^[0-9a-f]{64}$","type":"string"},"findings":{"items":{"additionalProperties":false,"properties":{"citations":{"items":{"additionalProperties":false,"properties":{"event_id":{"pattern":"^[0-9a-f]{32}$","type":"string"},"frame_digest":{"pattern":"^[0-9a-f]{64}$","type":"string"},"journal_position":{"minimum":1,"type":"integer"},"subject_id":{"pattern":"^[0-9a-f]{32}$","type":"string"}},"required":["subject_id","event_id","journal_position","frame_digest"],"type":"object"},"minItems":1,"type":"array"},"statement":{"maxLength":4096,"minLength":1,"type":"string"}},"required":["statement","citations"],"type":"object"},"type":"array"},"manifest_digest":{"pattern":"^[0-9a-f]{64}$","type":"string"},"manifest_id":{"pattern":"^[0-9a-f]{32}$","type":"string"},"recommendations":{"items":{"additionalProperties":false,"properties":{"affected_component_tags":{"items":{"maximum":30,"minimum":1,"type":"integer"},"minItems":1,"type":"array"},"citations":{"items":{"additionalProperties":false,"properties":{"event_id":{"pattern":"^[0-9a-f]{32}$","type":"string"},"frame_digest":{"pattern":"^[0-9a-f]{64}$","type":"string"},"journal_position":{"minimum":1,"type":"integer"},"subject_id":{"pattern":"^[0-9a-f]{32}$","type":"string"}},"required":["subject_id","event_id","journal_position","frame_digest"],"type":"object"},"minItems":1,"type":"array"},"statement":{"maxLength":4096,"minLength":1,"type":"string"}},"required":["statement","citations","affected_component_tags"],"type":"object"},"type":"array"},"schema_version":{"const":1,"type":"integer"}},"required":["schema_version","manifest_id","manifest_digest","deterministic_digest","findings","recommendations"],"type":"object"}"#;

/// Parses the closed proposal schema for a negotiated provider dialect.
///
/// # Errors
/// Returns a C5 schema/bound failure if protocol limits are narrower than the contract.
pub fn model_proposal_schema(
    dialect: SchemaDialect,
    limits: ProtocolLimits,
) -> Result<JsonSchema, DebuggerError> {
    JsonSchema::parse(MODEL_PROPOSAL_SCHEMA, dialect, JsonBounds::schema(limits)).map_err(protocol)
}

/// Converts each C6 segment into one separate C5 message without concatenating authority classes.
///
/// Evidence receives a fixed data delimiter and remains a separate user-role message. All other
/// segment bytes are passed unchanged after UTF-8 validation.
///
/// # Errors
/// Rejects non-UTF-8 context or C5 text/message bounds.
pub fn messages_from_render_plan(
    render: &RenderPlan,
    limits: ProtocolLimits,
) -> Result<Vec<Message>, DebuggerError> {
    let mut messages = Vec::with_capacity(render.segments().len());
    for segment in render.segments() {
        let role = match segment.message_role() {
            MessageRole::System => Role::System,
            MessageRole::Application => Role::Developer,
            MessageRole::User | MessageRole::Evidence => Role::User,
        };
        let source = String::from_utf8(segment.content().to_vec())
            .map_err(|_| invalid("C6 render segment is not UTF-8"))?;
        let text = if segment.message_role() == MessageRole::Evidence {
            format!(
                "<peritus-evidence source=\"{}\" digest=\"{}\">\n{}\n</peritus-evidence>",
                hex(segment.source_id().as_bytes()),
                hex(segment.digest().as_bytes()),
                source,
            )
        } else {
            source
        };
        let block = ContentBlock::Text(BoundedText::new(text, limits).map_err(protocol)?);
        messages.push(Message::new(role, vec![block], limits).map_err(protocol)?);
    }
    if messages.is_empty() {
        return Err(invalid("model render plan contains no messages"));
    }
    Ok(messages)
}

/// Immutable provider-neutral optional-analysis plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelAnalysisPlan {
    id: ModelAnalysisId,
    context_plan_id: ContextPlanId,
    render_digest: Sha256Digest,
    manifest_id: SelectionManifestId,
    manifest_digest: Sha256Digest,
    deterministic_digest: Sha256Digest,
    request: ModelRequest,
    request_digest: Sha256Digest,
    budget: ModelBudget,
    retry_policy: ModelRetryPolicy,
    protocol_limits: ProtocolLimits,
    digest: Sha256Digest,
}

impl ModelAnalysisPlan {
    /// Freezes a strict tool-free request against C6, selection, and deterministic analysis.
    ///
    /// `allow_provider_storage` must be explicitly true to permit provider persistence or caching.
    ///
    /// # Errors
    /// Rejects message drift, tools, parallel calls, non-strict output, remote persistence drift,
    /// or any model budget wider than the debugger job.
    #[allow(clippy::too_many_arguments, reason = "all frozen model bindings stay explicit")]
    pub fn new(
        id: ModelAnalysisId,
        context_plan_id: ContextPlanId,
        render: &RenderPlan,
        manifest: &TraceSelectionManifest,
        deterministic_digest: Sha256Digest,
        request: ModelRequest,
        debugger_limits: DebuggerLimits,
        budget: ModelBudget,
        retry_policy: ModelRetryPolicy,
        protocol_limits: ProtocolLimits,
        allow_provider_storage: bool,
    ) -> Result<Self, DebuggerError> {
        let expected_messages = messages_from_render_plan(render, protocol_limits)?;
        let StructuredOutput::JsonSchema { schema: structured_schema, strict: true, .. } =
            request.options().output()
        else {
            return Err(invalid("model request does not require the strict proposal schema"));
        };
        let expected_schema = model_proposal_schema(structured_schema.dialect(), protocol_limits)?;
        let persistence = request.options().persistence();
        let cache_is_local = matches!(request.options().cache(), CachePolicy::Disabled);
        if request.messages() != expected_messages
            || !request.tools().is_empty()
            || !matches!(request.tool_choice(), ToolChoice::None)
            || request.parallel_tool_policy() != ParallelToolPolicy::Disabled
            || structured_schema.digest() != expected_schema.digest()
            || !matches!(request.options().reasoning(), ReasoningPolicy::Disabled)
            || request.options().continuation().is_some()
            || !request.options().extensions().is_empty()
            || (!allow_provider_storage
                && (persistence.store() || persistence.background() || !cache_is_local))
        {
            return Err(invalid(
                "model request differs from the strict tool-free local-first E2 contract",
            ));
        }
        let request_bytes = request.canonical_bytes().map_err(protocol)?;
        debugger_limits.check(
            DebuggerLimit::ModelInputBytes,
            request_bytes.len(),
            DebuggerOperation::RunModelAnalysis,
        )?;
        if budget.max_events() > debugger_limits.get(DebuggerLimit::ModelEvents)
            || budget.max_output_bytes() > debugger_limits.get(DebuggerLimit::ModelOutputBytes)
            || budget.max_total_tokens() > debugger_limits.get(DebuggerLimit::ModelTokens)
            || u64::from(retry_policy.max_attempts()) > debugger_limits.model_attempts()
            || u64::from(retry_policy.max_attempts().saturating_sub(1)) > debugger_limits.retries()
        {
            return Err(budget_error("model plan exceeds the frozen debugger resource policy"));
        }
        let render_digest = render_digest(render);
        let request_digest = peritus_codec::sha256(&request_bytes);
        let mut value = Self {
            id,
            context_plan_id,
            render_digest,
            manifest_id: manifest.id(),
            manifest_digest: manifest.digest(),
            deterministic_digest,
            request,
            request_digest,
            budget,
            retry_policy,
            protocol_limits,
            digest: Sha256Digest::new([0; 32]),
        };
        value.digest = peritus_codec::sha256(&value.identity_bytes()?);
        Ok(value)
    }

    /// Stable model analysis identity.
    #[must_use]
    pub const fn id(&self) -> ModelAnalysisId {
        self.id
    }
    /// Complete frozen plan digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Frozen C5 semantic request digest.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }
    /// Exact request handed to the configured provider.
    #[must_use]
    pub const fn request(&self) -> &ModelRequest {
        &self.request
    }
    /// Selection manifest identity.
    #[must_use]
    pub const fn manifest_id(&self) -> SelectionManifestId {
        self.manifest_id
    }
    /// Selection manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Deterministic analysis digest.
    #[must_use]
    pub const fn deterministic_digest(&self) -> Sha256Digest {
        self.deterministic_digest
    }
    /// Exact attempt budget.
    #[must_use]
    pub const fn budget(&self) -> ModelBudget {
        self.budget
    }
    /// Exact retry policy.
    #[must_use]
    pub const fn retry_policy(&self) -> ModelRetryPolicy {
        self.retry_policy
    }
    /// C5 normalization limits.
    #[must_use]
    pub const fn protocol_limits(&self) -> ProtocolLimits {
        self.protocol_limits
    }

    fn identity_bytes(&self) -> Result<Vec<u8>, DebuggerError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(PLAN_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.context_plan_id.digest().as_bytes()).map_err(codec)?;
        writer.write_fixed(self.render_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.manifest_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.manifest_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.deterministic_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.request_digest.as_bytes()).map_err(codec)?;
        crate::aggregate::encode_model_budget(&mut writer, self.budget)?;
        crate::aggregate::encode_retry_policy(&mut writer, self.retry_policy)?;
        Ok(writer.into_bytes())
    }
}

fn render_digest(render: &RenderPlan) -> Sha256Digest {
    let mut bytes = b"peritus.debugger.render-plan.v1\0".to_vec();
    let accounting = render.accounting();
    for value in [
        accounting.context_window(),
        accounting.reserved_output(),
        accounting.reserved_protocol_overhead(),
        accounting.usable_input(),
        accounting.used_input(),
        accounting.remaining_input(),
    ] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(&(render.segments().len() as u64).to_be_bytes());
    for segment in render.segments() {
        bytes.extend_from_slice(segment.source_id().as_bytes());
        bytes.push(match segment.message_role() {
            MessageRole::System => 1,
            MessageRole::Application => 2,
            MessageRole::User => 3,
            MessageRole::Evidence => 4,
        });
        bytes.extend_from_slice(segment.digest().as_bytes());
        bytes.extend_from_slice(&(segment.content().len() as u64).to_be_bytes());
        bytes.extend_from_slice(segment.content());
    }
    peritus_codec::sha256(&bytes)
}

fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn protocol(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelProtocol,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::CorrectInput,
        error.to_string(),
    )
}
fn codec(error: impl core::fmt::Display) -> DebuggerError {
    protocol(error)
}
fn invalid(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelRejected,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
fn budget_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Budget,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
