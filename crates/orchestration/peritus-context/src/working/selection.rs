//! Working-entry projection through the existing C6 closure selector and typed renderer.

use peritus_codec::sha256;
use peritus_role::{ContextClass, MemoryVisibility, RoleProfile};
use std::collections::BTreeSet;
use crate::{AuthorityClass, ContentKind, ContextErrorKind, ContextGraph, ContextLimits, ContextNode, ContextNodeId,
    ContextNodeMetadata, ContextPlanId, Provenance, RenderPlan, RequirementMode, RoleVisibility,
    SelectionPolicy, TokenBudget, TrustClass, bind_context_content, build_render_plan, select_context};
use super::{
    ObservationId, WorkingBinding, WorkingEntry, WorkingEntryKind, WorkingEntryStatus,
    WorkingError, WorkingState,
};

/// Non-authoritative segments selected with complete entry dependencies and explicit omissions.
pub struct WorkingRenderView {
    plan: Option<RenderPlan>,
    omitted: Vec<ContextNodeId>,
    reconciliation: Option<WorkingSelectionReconciliation>,
}
impl WorkingRenderView {
    /// Typed evidence-only render plan, absent for an empty working model.
    #[must_use]
    pub const fn plan(&self) -> Option<&RenderPlan> { self.plan.as_ref() }
    /// Active-projection identifiers that did not fit; exact retained evidence remains available.
    #[must_use]
    pub fn omitted(&self) -> &[ContextNodeId] { &self.omitted }
    /// Exact required obligations represented only by compact retrieval references in this view.
    #[must_use]
    pub const fn reconciliation(&self) -> Option<&WorkingSelectionReconciliation> {
        self.reconciliation.as_ref()
    }
}

/// Recoverable selection state when exact required evidence cannot fit the current input window.
///
/// The referenced and deferred identifiers remain bound to `state_revision` and `required_digest`;
/// their exact content and lineage stay in the durable working state for focused retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingSelectionReconciliation {
    state_revision: u64,
    available_tokens: u64,
    minimum_required_tokens: Option<u64>,
    required_digest: [u8; 32],
    required: Vec<ContextNodeId>,
    referenced: Vec<ContextNodeId>,
}

impl WorkingSelectionReconciliation {
    /// Working-state revision that owns these obligations.
    #[must_use]
    pub const fn state_revision(&self) -> u64 { self.state_revision }
    /// Actual provider input headroom used for compact reference selection.
    #[must_use]
    pub const fn available_tokens(&self) -> u64 { self.available_tokens }
    /// Minimum required token count observed at the first closure that exceeded headroom.
    #[must_use]
    pub const fn minimum_required_tokens(&self) -> Option<u64> {
        self.minimum_required_tokens
    }
    /// Digest of the state revision and complete ordered required-closure identifiers.
    #[must_use]
    pub const fn required_digest(&self) -> [u8; 32] { self.required_digest }
    /// Complete required closure retained by the working state.
    #[must_use]
    pub fn required(&self) -> &[ContextNodeId] { &self.required }
    /// Required records represented in this prompt by digest-bound retrieval references.
    #[must_use]
    pub fn referenced(&self) -> &[ContextNodeId] { &self.referenced }
    /// Required records still awaiting focused retrieval in a later prompt.
    #[must_use]
    pub fn deferred(&self) -> Vec<ContextNodeId> {
        self.required.iter().copied()
            .filter(|id| self.referenced.binary_search(id).is_err())
            .collect()
    }
}

/// Renders current structured records, never a recursively summarized previous prompt.
///
/// Unresolved contradictions, failed approaches, and open plans are required roots. Their
/// prerequisites remain present even when stale. Every segment retains explicit status,
/// source handles and non-authoritative provenance. Literal requirements and pending operations
/// are separate host-owned pins and are not charged to this optional working-view allocation.
///
/// # Errors
/// Rejects scope drift, invalid graph/role projections, or zero capacity.
pub fn render_working_state(state: &WorkingState, binding: WorkingBinding, max_tokens: u64) -> Result<WorkingRenderView, WorkingError> {
    render_working_state_with_headroom(state, binding, max_tokens, max_tokens)
}

/// Renders working records with a preferred allocation and a hard available-input ceiling.
///
/// Only required roots and their complete dependencies may enlarge the preferred allocation.
/// Optional records cannot consume extra headroom. Stale and superseded records retire from the
/// ordinary graph but remain indexed and rejoin it when an active dependency needs them. No
/// stored entry or source is modified.
///
/// # Errors
/// Rejects the same invalid projections as [`render_working_state`]. Required closure that exceeds
/// `available_tokens` produces compact indexed references and typed reconciliation state. Hosts
/// must reserve complete request framing before calling.
pub fn render_working_state_with_headroom(state: &WorkingState, binding: WorkingBinding, preferred_tokens: u64, available_tokens: u64) -> Result<WorkingRenderView, WorkingError> {
    let entries = active_projection(state.entries(binding)?);
    if entries.is_empty() {
        return Ok(WorkingRenderView {
            plan: None,
            omitted: Vec::new(),
            reconciliation: None,
        });
    }
    let profile = RoleProfile::for_harness_role(binding.role());
    if profile.context().memory_visibility() == MemoryVisibility::Excluded {
        return Ok(WorkingRenderView {
            plan: None,
            omitted: entries.iter().map(|entry| entry.id()).collect(),
            reconciliation: None,
        });
    }
    let limits = ContextLimits::new(entries.len().max(1), 32_768, state.limits().links(), 5)
        .map_err(|_| WorkingError::InvalidLimit)?;
    let mut identity = b"peritus-working-render-v1".to_vec();
    identity.extend_from_slice(binding.run().as_bytes());
    identity.extend_from_slice(binding.workspace().as_bytes());
    identity.extend_from_slice(binding.task().as_bytes());
    identity.extend_from_slice(format!("{:?}", binding.role()).as_bytes());
    identity.extend_from_slice(&binding.conversation_revision().to_be_bytes());
    identity.extend_from_slice(&state.revision().to_be_bytes());
    let mut nodes = Vec::with_capacity(entries.len());
    for entry in &entries {
        match node(entry, binding, limits) {
            Ok(node) => nodes.push(node),
            Err(WorkingError::Capacity) => {
                return build_reference_view(
                    state,
                    &entries,
                    limits,
                    profile,
                    available_tokens,
                    identity,
                    None,
                );
            }
            Err(error) => return Err(error),
        }
    }
    let graph = ContextGraph::new(nodes, limits).map_err(|_| WorkingError::DependencyCycle)?;
    let mut allocation = preferred_tokens.min(available_tokens);
    let mut required_overflow = None;
    let plan = loop {
        let budget = TokenBudget::new(allocation, 0, 0).map_err(|_| WorkingError::Capacity)?;
        let bytes = usize::try_from(allocation.saturating_mul(3)).map_err(|_| WorkingError::Capacity)?;
        let policy = SelectionPolicy::new(profile.clone(), budget, entries.len(), bytes)
            .map_err(|_| WorkingError::Capacity)?;
        let mut plan_identity = identity.clone();
        plan_identity.extend_from_slice(&allocation.to_be_bytes());
        match select_context(&graph, &policy, ContextPlanId::new(sha256(&plan_identity))) {
            Ok(plan) => break Some(plan),
            Err(error) if error.kind() == ContextErrorKind::RequiredTokenBudgetExceeded => {
                let needed = error.actual().ok_or(WorkingError::Capacity)?;
                if needed <= allocation || needed > available_tokens {
                    required_overflow = Some(Some(needed));
                    break None;
                }
                // Each retry admits at least one more required closure; optional selection has
                // not begun. The existing selector owns dependency and accounting semantics.
                allocation = needed;
            }
            Err(error) if matches!(
                error.kind(),
                ContextErrorKind::RequiredByteLimitExceeded
                    | ContextErrorKind::RequiredNodeLimitExceeded
            ) => {
                required_overflow = Some(None);
                break None;
            }
            Err(_) => return Err(WorkingError::Capacity),
        }
    };
    if let Some(plan) = plan {
        let omitted = entries.iter().filter(|entry| !plan.contains(entry.id()))
            .map(|entry| entry.id()).collect();
        let rendered = build_render_plan(&graph, &plan).map_err(|_| WorkingError::BindingMismatch)?;
        return Ok(WorkingRenderView { plan: Some(rendered), omitted, reconciliation: None });
    }

    build_reference_view(
        state,
        &entries,
        limits,
        profile,
        available_tokens,
        identity,
        required_overflow.flatten(),
    )
}

fn build_reference_view(
    state: &WorkingState,
    entries: &[&WorkingEntry],
    limits: ContextLimits,
    profile: RoleProfile,
    available_tokens: u64,
    identity: Vec<u8>,
    minimum_required_tokens: Option<u64>,
) -> Result<WorkingRenderView, WorkingError> {
    let required = required_closure(entries);
    let required_digest = required_digest(&identity, &required);
    let mut reference_nodes = Vec::with_capacity(required.len());
    for entry in entries.iter().filter(|entry| required.binary_search(&entry.id()).is_ok()) {
        reference_nodes.push(reference_node(entry, state.binding(), limits)?);
    }
    let reference_graph = ContextGraph::new(reference_nodes, limits)
        .map_err(|_| WorkingError::DependencyCycle)?;
    let budget = TokenBudget::new(available_tokens, 0, 0).map_err(|_| WorkingError::Capacity)?;
    let bytes = usize::try_from(available_tokens.saturating_mul(3))
        .map_err(|_| WorkingError::Capacity)?;
    let policy = SelectionPolicy::new(profile, budget, required.len().max(1), bytes)
        .map_err(|_| WorkingError::Capacity)?;
    let mut reference_identity = identity;
    reference_identity.extend_from_slice(b"\0required-references-v1\0");
    reference_identity.extend_from_slice(&available_tokens.to_be_bytes());
    let reference_plan = select_context(
        &reference_graph,
        &policy,
        ContextPlanId::new(sha256(&reference_identity)),
    ).map_err(|_| WorkingError::Capacity)?;
    let referenced = required.iter().copied()
        .filter(|id| reference_plan.contains(*id))
        .collect::<Vec<_>>();
    let omitted = entries.iter().filter(|entry| !reference_plan.contains(entry.id()))
        .map(|entry| entry.id())
        .collect();
    let rendered = build_render_plan(&reference_graph, &reference_plan)
        .map_err(|_| WorkingError::BindingMismatch)?;
    let reconciliation = WorkingSelectionReconciliation {
        state_revision: state.revision(),
        available_tokens,
        minimum_required_tokens,
        required_digest,
        required,
        referenced,
    };
    Ok(WorkingRenderView {
        plan: Some(rendered),
        omitted,
        reconciliation: Some(reconciliation),
    })
}

fn node(entry: &WorkingEntry, binding: WorkingBinding, limits: ContextLimits) -> Result<ContextNode, WorkingError> {
    let rendered = render(entry);
    let bytes = rendered.into_bytes();
    let tokens = (bytes.len() as u64).saturating_add(2) / 3;
    let content = bind_context_content(bytes.clone(), sha256(&bytes), limits).map_err(|_| WorkingError::Capacity)?;
    let visibility = RoleVisibility::new(vec![binding.role().actor_role()], limits).map_err(|_| WorkingError::BindingMismatch)?;
    let important = is_required_root(entry);
    let requirement = if important { RequirementMode::Required } else { RequirementMode::Optional };
    let recency = entry.links().supports().iter().chain(entry.links().contradicts()).map(|source| source.get()).max().unwrap_or(0);
    let metadata = ContextNodeMetadata::new(entry.id(), Provenance::Memory, AuthorityClass::NonAuthoritative, TrustClass::Untrusted,
        ContextClass::MemoryEvidence, ContentKind::MemoryEvidence, tokens, recency, requirement, 0, visibility,
        entry.links().depends_on().to_vec(), limits).map_err(|_| WorkingError::Capacity)?;
    Ok(ContextNode::new(metadata, content))
}

fn reference_node(
    entry: &WorkingEntry,
    binding: WorkingBinding,
    limits: ContextLimits,
) -> Result<ContextNode, WorkingError> {
    let rendered = render_reference(entry);
    let bytes = rendered.into_bytes();
    let tokens = (bytes.len() as u64).saturating_add(2) / 3;
    let content = bind_context_content(bytes.clone(), sha256(&bytes), limits)
        .map_err(|_| WorkingError::Capacity)?;
    let visibility = RoleVisibility::new(vec![binding.role().actor_role()], limits)
        .map_err(|_| WorkingError::BindingMismatch)?;
    let recency = entry.links().supports().iter().chain(entry.links().contradicts())
        .map(|source| source.get()).max().unwrap_or(0);
    let metadata = ContextNodeMetadata::new(
        entry.id(),
        Provenance::Memory,
        AuthorityClass::NonAuthoritative,
        TrustClass::Untrusted,
        ContextClass::MemoryEvidence,
        ContentKind::MemoryEvidence,
        tokens,
        recency,
        RequirementMode::Optional,
        u16::from(is_required_root(entry)),
        visibility,
        entry.links().depends_on().to_vec(),
        limits,
    ).map_err(|_| WorkingError::Capacity)?;
    Ok(ContextNode::new(metadata, content))
}

fn is_required_root(entry: &WorkingEntry) -> bool {
    !entry.status().is_retired() && (
        entry.status() == WorkingEntryStatus::Contradicted
            || !entry.links().contradicts().is_empty()
            || (entry.kind() == WorkingEntryKind::FailedApproach
                && entry.status() != WorkingEntryStatus::Resolved)
            || (entry.kind() == WorkingEntryKind::Plan
                && entry.status() != WorkingEntryStatus::Resolved)
    )
}

fn active_projection(entries: &[WorkingEntry]) -> Vec<&WorkingEntry> {
    let mut included = entries.iter()
        .filter(|entry| !entry.status().is_retired())
        .map(WorkingEntry::id)
        .collect::<BTreeSet<_>>();
    let mut frontier = included.iter().copied().collect::<Vec<_>>();
    let mut cursor = 0;
    while cursor < frontier.len() {
        let id = frontier[cursor];
        cursor += 1;
        let Ok(index) = entries.binary_search_by_key(&id, WorkingEntry::id) else {
            continue;
        };
        for dependency in entries[index].links().depends_on() {
            if included.insert(*dependency) { frontier.push(*dependency); }
        }
    }
    entries.iter().filter(|entry| included.contains(&entry.id())).collect()
}

fn required_closure(entries: &[&WorkingEntry]) -> Vec<ContextNodeId> {
    let mut required = entries.iter().filter(|entry| is_required_root(entry))
        .map(|entry| entry.id())
        .collect::<BTreeSet<_>>();
    let mut frontier = required.iter().copied().collect::<Vec<_>>();
    let mut cursor = 0;
    while cursor < frontier.len() {
        let id = frontier[cursor];
        cursor += 1;
        let Ok(index) = entries.binary_search_by_key(&id, |entry| entry.id()) else {
            continue;
        };
        for dependency in entries[index].links().depends_on() {
            if required.insert(*dependency) {
                frontier.push(*dependency);
            }
        }
    }
    required.into_iter().collect()
}

fn required_digest(identity: &[u8], required: &[ContextNodeId]) -> [u8; 32] {
    let mut bytes = b"peritus-working-required-v1".to_vec();
    bytes.extend_from_slice(identity);
    for id in required {
        bytes.extend_from_slice(id.as_bytes());
    }
    sha256(&bytes).into_bytes()
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

fn render_reference(entry: &WorkingEntry) -> String {
    use core::fmt::Write as _;
    let exact = sha256(render(entry).as_bytes()).into_bytes();
    let mut identifier = String::new();
    for byte in entry.id().as_bytes() { let _ = write!(identifier, "{byte:02x}"); }
    let mut digest = String::new();
    for byte in exact { let _ = write!(digest, "{byte:02x}"); }
    let mut text = format!(
        "entry:{identifier}; kind={:?}; status={:?}; full_render_sha256={digest}; retrieval=context_read.entry_ids\n",
        entry.kind(),
        entry.status(),
    );
    let supports = observation_ids_digest(b"supports", entry.links().supports());
    let contradicts = observation_ids_digest(b"contradicts", entry.links().contradicts());
    let dependencies = entry_ids_digest(b"dependencies", entry.links().depends_on());
    let _ = writeln!(
        text,
        "supports_count={}; supports_sha256={}; contradicts_count={}; contradicts_sha256={}; dependencies_count={}; dependencies_sha256={}",
        entry.links().supports().len(),
        hex_digest(supports),
        entry.links().contradicts().len(),
        hex_digest(contradicts),
        entry.links().depends_on().len(),
        hex_digest(dependencies),
    );
    text
}

fn observation_ids_digest(domain: &[u8], ids: &[ObservationId]) -> [u8; 32] {
    let mut bytes = b"peritus-working-observation-refs-v1\0".to_vec();
    bytes.extend_from_slice(domain);
    for id in ids { bytes.extend_from_slice(&id.get().to_be_bytes()); }
    sha256(&bytes).into_bytes()
}

fn entry_ids_digest(domain: &[u8], ids: &[ContextNodeId]) -> [u8; 32] {
    let mut bytes = b"peritus-working-entry-refs-v1\0".to_vec();
    bytes.extend_from_slice(domain);
    for id in ids { bytes.extend_from_slice(id.as_bytes()); }
    sha256(&bytes).into_bytes()
}

fn hex_digest(bytes: [u8; 32]) -> String {
    use core::fmt::Write as _;
    let mut digest = String::with_capacity(64);
    for byte in bytes { let _ = write!(digest, "{byte:02x}"); }
    digest
}
