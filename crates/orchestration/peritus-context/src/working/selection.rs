//! Working-entry projection through the existing C6 closure selector and typed renderer.

use peritus_codec::sha256;
use peritus_role::{ContextClass, MemoryVisibility, RoleProfile};
use crate::{AuthorityClass, ContentKind, ContextGraph, ContextLimits, ContextNode, ContextNodeId,
    ContextNodeMetadata, ContextPlanId, Provenance, RenderPlan, RequirementMode, RoleVisibility,
    SelectionPolicy, TokenBudget, TrustClass, bind_context_content, build_render_plan, select_context};
use super::{WorkingBinding, WorkingEntry, WorkingEntryKind, WorkingEntryStatus, WorkingError, WorkingState};

/// Non-authoritative segments selected with complete entry dependencies and explicit omissions.
pub struct WorkingRenderView {
    plan: Option<RenderPlan>,
    omitted: Vec<ContextNodeId>,
}
impl WorkingRenderView {
    /// Typed evidence-only render plan, absent for an empty working model.
    #[must_use]
    pub const fn plan(&self) -> Option<&RenderPlan> { self.plan.as_ref() }
    /// Entry identifiers that did not fit; exact source evidence remains locally available.
    #[must_use]
    pub fn omitted(&self) -> &[ContextNodeId] { &self.omitted }
}

/// Renders current structured records, never a recursively summarized previous prompt.
///
/// Unresolved contradictions, failed approaches, and open plans are required roots. Their
/// prerequisites remain present even when stale. Every segment retains explicit status,
/// source handles and non-authoritative provenance. Literal requirements and pending operations
/// are separate host-owned pins and are not charged to this optional working-view allocation.
///
/// # Errors
/// Rejects scope drift, invalid graph/role projections, zero capacity, or required-root overflow.
pub fn render_working_state(state: &WorkingState, binding: WorkingBinding, max_tokens: u64) -> Result<WorkingRenderView, WorkingError> {
    let entries = state.entries(binding)?;
    if entries.is_empty() { return Ok(WorkingRenderView { plan: None, omitted: Vec::new() }); }
    let profile = RoleProfile::for_harness_role(binding.role());
    if profile.context().memory_visibility() == MemoryVisibility::Excluded {
        return Ok(WorkingRenderView { plan: None, omitted: entries.iter().map(WorkingEntry::id).collect() });
    }
    let limits = ContextLimits::new(state.limits().entries(), 32_768, state.limits().links(), 5)
        .map_err(|_| WorkingError::InvalidLimit)?;
    let mut nodes = Vec::with_capacity(entries.len());
    for entry in entries { nodes.push(node(entry, binding, limits)?); }
    let graph = ContextGraph::new(nodes, limits).map_err(|_| WorkingError::DependencyCycle)?;
    let budget = TokenBudget::new(max_tokens, 0, 0).map_err(|_| WorkingError::Capacity)?;
    let bytes = usize::try_from(max_tokens.saturating_mul(3)).map_err(|_| WorkingError::Capacity)?;
    let policy = SelectionPolicy::new(profile, budget, state.limits().entries(), bytes).map_err(|_| WorkingError::Capacity)?;
    let mut identity = b"peritus-working-render-v1".to_vec();
    identity.extend_from_slice(binding.run().as_bytes());
    identity.extend_from_slice(binding.workspace().as_bytes());
    identity.extend_from_slice(binding.task().as_bytes());
    identity.extend_from_slice(format!("{:?}", binding.role()).as_bytes());
    identity.extend_from_slice(&binding.conversation_revision().to_be_bytes());
    identity.extend_from_slice(&state.revision().to_be_bytes());
    identity.extend_from_slice(&max_tokens.to_be_bytes());
    let plan = select_context(&graph, &policy, ContextPlanId::new(sha256(&identity))).map_err(|_| WorkingError::Capacity)?;
    let omitted = entries.iter().filter(|entry| !plan.contains(entry.id())).map(WorkingEntry::id).collect();
    let rendered = build_render_plan(&graph, &plan).map_err(|_| WorkingError::BindingMismatch)?;
    Ok(WorkingRenderView { plan: Some(rendered), omitted })
}

fn node(entry: &WorkingEntry, binding: WorkingBinding, limits: ContextLimits) -> Result<ContextNode, WorkingError> {
    let rendered = render(entry);
    let bytes = rendered.into_bytes();
    let tokens = (bytes.len() as u64).saturating_add(2) / 3;
    let content = bind_context_content(bytes.clone(), sha256(&bytes), limits).map_err(|_| WorkingError::Capacity)?;
    let visibility = RoleVisibility::new(vec![binding.role().actor_role()], limits).map_err(|_| WorkingError::BindingMismatch)?;
    let important = entry.status() != WorkingEntryStatus::Superseded && (
        entry.status() == WorkingEntryStatus::Contradicted
        || !entry.links().contradicts().is_empty()
        || (entry.kind() == WorkingEntryKind::FailedApproach && entry.status() != WorkingEntryStatus::Resolved)
        || (entry.kind() == WorkingEntryKind::Plan && entry.status() != WorkingEntryStatus::Resolved));
    let requirement = if important { RequirementMode::Required } else { RequirementMode::Optional };
    let recency = entry.links().supports().iter().chain(entry.links().contradicts()).map(|source| source.get()).max().unwrap_or(0);
    let metadata = ContextNodeMetadata::new(entry.id(), Provenance::Memory, AuthorityClass::NonAuthoritative, TrustClass::Untrusted,
        ContextClass::MemoryEvidence, ContentKind::MemoryEvidence, tokens, recency, requirement, 0, visibility,
        entry.links().depends_on().to_vec(), limits).map_err(|_| WorkingError::Capacity)?;
    Ok(ContextNode::new(metadata, content))
}

fn render(entry: &WorkingEntry) -> String {
    use core::fmt::Write as _;
    let mut identifier=String::new();
    for byte in entry.id().as_bytes() { let _=write!(identifier,"{byte:02x}"); }
    let mut text = format!("entry:{identifier}; kind={:?}; status={:?}; authority=none\ntext={:?}\n", entry.kind(), entry.status(), String::from_utf8_lossy(entry.content().bytes()));
    for (label, sources) in [("supports", entry.links().supports()), ("contradicts", entry.links().contradicts())] {
        let _ = write!(text, "{label}:");
        for source in sources { let _ = write!(text, " obs:{:06}", source.get()); }
        text.push('\n');
    }
    let _ = write!(text, "dependencies={:?}; supersedes={:?}; validity={:?}", entry.links().depends_on(), entry.supersedes(), entry.validity());
    text.push('\n');
    text
}
