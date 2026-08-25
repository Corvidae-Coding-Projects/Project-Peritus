//! C6 role, memory, context-selection, and provider-neutral rendering composition.

use core::fmt::Write as _;

use peritus_context::{
    AuthorityClass, ContentKind, ContextError, ContextGraph, ContextNode, ContextNodeId,
    ContextNodeMetadata, ContextPlan, ContextPlanId, Provenance, RenderPlan, RequirementMode,
    RoleVisibility, SelectionPolicy, TokenBudget, TrustClass, bind_context_content,
    build_render_plan, select_context,
};
use peritus_memory::{
    MemoryCandidate, MemoryError, MemoryRecord, MemoryTombstone, RetrievalPlan, RetrievalPolicy,
    RetrievalQuery, SourceProvenance, retrieve,
};
use peritus_model_protocol::{
    BoundedText, ContentBlock, Message, ProtocolError, ProtocolLimits, Role,
};
use peritus_policy::ActorRole;
use peritus_role::{ContextClass, RoleProfile};

/// Optional C6 memory retrieval inputs for one context cycle.
#[derive(Clone, Copy, Debug)]
pub struct MemorySelection<'a> {
    records: &'a [MemoryRecord],
    tombstones: &'a [MemoryTombstone],
    policy: &'a RetrievalPolicy,
    query: &'a RetrievalQuery,
    delimiter_tokens: u32,
    recency_sequence: u64,
    priority: u16,
}

impl<'a> MemorySelection<'a> {
    /// Binds canonical memory inputs and explicit materialization accounting.
    #[allow(clippy::too_many_arguments, reason = "memory retrieval inputs remain explicit")]
    #[must_use]
    pub const fn new(
        records: &'a [MemoryRecord],
        tombstones: &'a [MemoryTombstone],
        policy: &'a RetrievalPolicy,
        query: &'a RetrievalQuery,
        delimiter_tokens: u32,
        recency_sequence: u64,
        priority: u16,
    ) -> Self {
        Self { records, tombstones, policy, query, delimiter_tokens, recency_sequence, priority }
    }
}

/// Fully selected C6 context and memory outcome for one model request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPreparation {
    graph: ContextGraph,
    memory: Option<RetrievalPlan>,
    plan: ContextPlan,
    render: RenderPlan,
}

impl ContextPreparation {
    /// Borrows the graph after selected memory was materialized as evidence nodes.
    #[must_use]
    pub const fn graph(&self) -> &ContextGraph {
        &self.graph
    }
    /// Borrows the explainable memory result when retrieval was requested.
    #[must_use]
    pub const fn memory(&self) -> Option<&RetrievalPlan> {
        self.memory.as_ref()
    }
    /// Borrows the deterministic context selection plan.
    #[must_use]
    pub const fn plan(&self) -> &ContextPlan {
        &self.plan
    }
    /// Borrows provider-neutral typed render segments.
    #[must_use]
    pub const fn render(&self) -> &RenderPlan {
        &self.render
    }
}

/// Typed failure from D0 context preparation or C5 message projection.
#[derive(Debug)]
#[non_exhaustive]
pub enum ContextDriveError {
    /// The memory query was prepared for a different canonical role.
    MemoryRoleMismatch,
    /// Checked token accounting overflowed while adding evidence delimiters.
    TokenAccountingOverflow,
    /// Context content was not provider-neutral UTF-8 text.
    NonUtf8Segment,
    /// C6 context planning or materialization failed.
    Context(ContextError),
    /// C6 memory retrieval failed.
    Memory(MemoryError),
    /// C5 message construction failed.
    Protocol(ProtocolError),
}

impl core::fmt::Display for ContextDriveError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MemoryRoleMismatch => {
                formatter.write_str("memory query role does not match the agent role")
            }
            Self::TokenAccountingOverflow => {
                formatter.write_str("memory delimiter token accounting overflowed")
            }
            Self::NonUtf8Segment => {
                formatter.write_str("context render segment is not UTF-8 model text")
            }
            Self::Context(error) => {
                write!(formatter, "context planning failed: {:?}", error.kind())
            }
            Self::Memory(error) => write!(
                formatter,
                "memory planning failed: {:?} at {:?}",
                error.kind(),
                error.field_value(),
            ),
            Self::Protocol(error) => core::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ContextDriveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Protocol(error) => Some(error),
            Self::Context(_)
            | Self::Memory(_)
            | Self::MemoryRoleMismatch
            | Self::TokenAccountingOverflow
            | Self::NonUtf8Segment => None,
        }
    }
}

impl From<ContextError> for ContextDriveError {
    fn from(value: ContextError) -> Self {
        Self::Context(value)
    }
}

impl From<MemoryError> for ContextDriveError {
    fn from(value: MemoryError) -> Self {
        Self::Memory(value)
    }
}

impl From<ProtocolError> for ContextDriveError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

/// Retrieves scoped memory, materializes it as non-authoritative C6 evidence, and selects/render
/// plans for the exact B1 role.
///
/// # Errors
///
/// Returns typed memory, graph, selection, content, role-binding, or checked-accounting failures.
#[allow(clippy::too_many_arguments, reason = "the complete context-cycle boundary is explicit")]
pub fn prepare_context(
    base_graph: &ContextGraph,
    actor_role: ActorRole,
    plan_id: ContextPlanId,
    token_budget: TokenBudget,
    max_selected_nodes: usize,
    max_selected_bytes: usize,
    memory: Option<MemorySelection<'_>>,
) -> Result<ContextPreparation, ContextDriveError> {
    let role = RoleProfile::for_actor_role(actor_role);
    let limits = base_graph.limits();
    let mut nodes = base_graph.nodes().to_vec();
    let memory_plan = if let Some(inputs) = memory {
        if inputs.query.role() != &role {
            return Err(ContextDriveError::MemoryRoleMismatch);
        }
        let plan = retrieve(inputs.records, inputs.tombstones, inputs.policy, inputs.query)?;
        for candidate in plan.selected() {
            nodes.push(memory_node(candidate, actor_role, inputs, limits)?);
        }
        Some(plan)
    } else {
        None
    };
    nodes.sort_by_key(ContextNode::id);
    let graph = ContextGraph::new(nodes, limits)?;
    let selection =
        SelectionPolicy::new(role, token_budget, max_selected_nodes, max_selected_bytes)?;
    let plan = select_context(&graph, &selection, plan_id)?;
    let render = build_render_plan(&graph, &plan)?;
    Ok(ContextPreparation { graph, memory: memory_plan, plan, render })
}

/// Converts every typed C6 render segment into one separately delimited C5 message.
///
/// The caller must reserve provider-protocol tokens for these delimiters in its C6 token budget.
/// Evidence remains a user-role quoted block; it never becomes system or developer authority.
///
/// # Errors
///
/// Rejects non-UTF-8 context or any C5 message/content bound violation.
pub fn render_messages(
    render: &RenderPlan,
    limits: ProtocolLimits,
) -> Result<Vec<Message>, ContextDriveError> {
    render
        .segments()
        .iter()
        .map(|segment| {
            let content = core::str::from_utf8(segment.content())
                .map_err(|_| ContextDriveError::NonUtf8Segment)?;
            let mut value = String::from("<peritus-context source=\"");
            value.push_str(&hex(segment.source_id().as_bytes()));
            value.push_str("\" class=\"");
            value.push_str(context_class(segment.context_class()));
            value.push_str("\" provenance=\"");
            value.push_str(provenance(segment.provenance()));
            value.push_str("\" authority=\"");
            value.push_str(authority(segment.authority()));
            value.push_str("\" trust=\"");
            value.push_str(trust(segment.trust()));
            value.push_str("\" digest=\"");
            value.push_str(&digest_hex(segment.digest().as_bytes()));
            value.push_str("\">\n");
            value.push_str(content);
            value.push_str("\n</peritus-context>");
            let role = match segment.message_role() {
                peritus_context::MessageRole::System => Role::System,
                peritus_context::MessageRole::Application => Role::Developer,
                peritus_context::MessageRole::User | peritus_context::MessageRole::Evidence => {
                    Role::User
                }
            };
            let text = BoundedText::new(value, limits)?;
            Message::new(role, vec![ContentBlock::Text(text)], limits).map_err(Into::into)
        })
        .collect()
}

fn memory_node(
    candidate: &MemoryCandidate,
    actor_role: ActorRole,
    inputs: MemorySelection<'_>,
    limits: peritus_context::ContextLimits,
) -> Result<ContextNode, ContextDriveError> {
    let token_estimate = candidate
        .estimated_tokens()
        .checked_add(inputs.delimiter_tokens)
        .ok_or(ContextDriveError::TokenAccountingOverflow)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MemoryCandidate::quote_open());
    bytes.extend_from_slice(b"\nsource-provenance: ");
    bytes.extend_from_slice(memory_provenance(candidate.material().provenance()).as_bytes());
    bytes.extend_from_slice(b"\n");
    bytes.extend_from_slice(candidate.material().content());
    bytes.extend_from_slice(b"\n");
    bytes.extend_from_slice(MemoryCandidate::quote_close());
    let digest = peritus_codec::sha256(&bytes);
    let id = ContextNodeId::new(candidate.id().into_bytes())?;
    let visibility = RoleVisibility::new(vec![actor_role], limits)?;
    let metadata = ContextNodeMetadata::new(
        id,
        Provenance::Memory,
        AuthorityClass::NonAuthoritative,
        TrustClass::Untrusted,
        ContextClass::MemoryEvidence,
        ContentKind::MemoryEvidence,
        u64::from(token_estimate),
        inputs.recency_sequence,
        RequirementMode::Optional,
        inputs.priority,
        visibility,
        Vec::new(),
        limits,
    )?;
    let content = bind_context_content(bytes, digest, limits)?;
    Ok(ContextNode::new(metadata, content))
}

const fn memory_provenance(value: SourceProvenance) -> &'static str {
    match value {
        SourceProvenance::Repository => "repository",
        SourceProvenance::Tool => "tool",
        SourceProvenance::Provider => "provider",
        SourceProvenance::External => "external",
        SourceProvenance::Agent => "agent",
        SourceProvenance::Review => "review",
        SourceProvenance::User => "user",
    }
}

const fn context_class(value: ContextClass) -> &'static str {
    match value {
        ContextClass::ImmutablePolicy => "immutable-policy",
        ContextClass::AcceptanceSpecification => "acceptance-specification",
        ContextClass::ActiveUserRequest => "active-user-request",
        ContextClass::RepositoryInstructions => "repository-instructions",
        ContextClass::RepositorySource => "repository-source",
        ContextClass::CandidateDiff => "candidate-diff",
        ContextClass::WorkspaceState => "workspace-state",
        ContextClass::GateEvidence => "gate-evidence",
        ContextClass::ToolObservation => "tool-observation",
        ContextClass::MemoryEvidence => "memory-evidence",
        ContextClass::PriorFinding => "prior-finding",
        ContextClass::FindingResolution => "finding-resolution",
        ContextClass::AgentProgress => "agent-progress",
        ContextClass::HiddenReasoning => "hidden-reasoning",
    }
}

const fn provenance(value: Provenance) -> &'static str {
    match value {
        Provenance::System => "system",
        Provenance::Application => "application",
        Provenance::User => "user",
        Provenance::Repository => "repository",
        Provenance::External => "external",
        Provenance::Memory => "memory",
        Provenance::Tool => "tool",
        Provenance::Agent => "agent",
        Provenance::Review => "review",
        Provenance::DerivedCompaction => "derived-compaction",
    }
}

const fn authority(value: AuthorityClass) -> &'static str {
    match value {
        AuthorityClass::SystemPolicy => "system-policy",
        AuthorityClass::ApplicationPolicy => "application-policy",
        AuthorityClass::AcceptanceSpecification => "acceptance-specification",
        AuthorityClass::UserInstruction => "user-instruction",
        AuthorityClass::NonAuthoritative => "non-authoritative",
    }
}

const fn trust(value: TrustClass) -> &'static str {
    match value {
        TrustClass::Trusted => "trusted",
        TrustClass::Constrained => "constrained",
        TrustClass::Untrusted => "untrusted",
    }
}

fn hex(bytes: &[u8; 16]) -> String {
    encode_hex(bytes)
}

fn digest_hex(bytes: &[u8; 32]) -> String {
    encode_hex(bytes)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}
