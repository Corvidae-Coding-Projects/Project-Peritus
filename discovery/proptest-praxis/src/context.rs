//! Independent operation histories for the public task-local working-context API.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use peritus_context::working::{
    ObservationId, ObservationKind, ObservationSource, WorkingBinding, WorkingDelta, WorkingEntry,
    WorkingEntryKind, WorkingEntryStatus, WorkingEnvironment, WorkingError, WorkingLimits,
    WorkingLinks, WorkingRenderView, WorkingState, WorkingValidity, apply_working_delta,
    ingest_working_observation, render_working_state, render_working_state_complete,
    render_working_state_with_headroom,
};
use peritus_context::{
    AuthorityClass, ContextErrorKind, ContextLimits, ContextNodeId, MessageRole, Provenance,
    TrustClass, bind_context_content,
};
use peritus_role::HarnessRole;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

/// Exact logical case; persisted cases do not depend on future strategy/seed behavior.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub role: Role,
    pub nodes: Vec<Node>,
    pub batch_insert: bool,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Role {
    Writer,
    Reviewer,
    Fixer,
    Evaluator,
    Evolver,
}

impl Role {
    fn production(self) -> HarnessRole {
        match self {
            Self::Writer => HarnessRole::Writer,
            Self::Reviewer => HarnessRole::Reviewer,
            Self::Fixer => HarnessRole::Fixer,
            Self::Evaluator => HarnessRole::Evaluator,
            Self::Evolver => HarnessRole::Evolver,
        }
    }

    fn excludes_memory(self) -> bool {
        matches!(self, Self::Reviewer)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub dependencies: Vec<usize>,
    pub importance: Importance,
    pub text: String,
}

/// Input-level truth table, independent of the production `important` calculation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Importance {
    Optional,
    OpenPlan,
    FailedApproach,
    Contradicted,
    ResolvedPlan,
}

impl Importance {
    fn required(self) -> bool {
        matches!(self, Self::OpenPlan | Self::FailedApproach | Self::Contradicted)
    }

    fn production(self) -> (WorkingEntryKind, WorkingEntryStatus) {
        match self {
            Self::Optional => (WorkingEntryKind::Assertion, WorkingEntryStatus::Open),
            Self::OpenPlan => (WorkingEntryKind::Plan, WorkingEntryStatus::Open),
            Self::FailedApproach => (WorkingEntryKind::FailedApproach, WorkingEntryStatus::Open),
            Self::Contradicted => (WorkingEntryKind::Hypothesis, WorkingEntryStatus::Contradicted),
            Self::ResolvedPlan => (WorkingEntryKind::Plan, WorkingEntryStatus::Resolved),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Basis {
    RequiredClosure,
    AllEntries,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Probe { basis: Basis, preferred_delta: i8, available_delta: i8, headroom: bool },
    Replace { index: usize, importance: Importance, text: String },
    RejectRevision,
    RejectForeignBinding,
    RejectDependencyCycle,
}

fn importance_strategy() -> BoxedStrategy<Importance> {
    prop_oneof![
        Just(Importance::Optional),
        Just(Importance::OpenPlan),
        Just(Importance::FailedApproach),
        Just(Importance::Contradicted),
        Just(Importance::ResolvedPlan),
    ]
    .boxed()
}

fn text_strategy() -> BoxedStrategy<String> {
    prop::collection::vec(
        prop_oneof![Just('x'), Just('é'), Just('\u{301}'), Just('🦀'), Just('"'), Just('\\')],
        1..=40,
    )
    .prop_map(|chars| chars.into_iter().collect())
    .boxed()
}

/// Constructive DAG generation; shrinking rebuilds references instead of skipping operations.
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let max_nodes = max_steps.clamp(4, 16);
    let operation = prop_oneof![
        5 => (any::<bool>(), -1_i8..=1, -1_i8..=1, any::<bool>()).prop_map(
            |(all, preferred_delta, available_delta, headroom)| Operation::Probe {
                basis: if all { Basis::AllEntries } else { Basis::RequiredClosure },
                preferred_delta,
                available_delta,
                headroom,
            },
        ),
        4 => (0_usize..max_nodes, importance_strategy(), text_strategy()).prop_map(
            |(index, importance, text)| Operation::Replace { index, importance, text },
        ),
        1 => Just(Operation::RejectRevision),
        1 => Just(Operation::RejectForeignBinding),
        1 => Just(Operation::RejectDependencyCycle),
    ];
    (
        0_u8..5,
        0_u8..4,
        prop::collection::vec(
            (any::<u16>(), importance_strategy(), text_strategy()),
            1..=max_nodes,
        ),
        any::<bool>(),
        prop::collection::vec(operation, 0..=max_steps.min(256)),
    )
        .prop_map(|(role, shape, raw, batch_insert, operations)| {
            let mut nodes: Vec<_> = raw
                .into_iter()
                .enumerate()
                .map(|(index, (edges, importance, text))| {
                    let dependencies = match shape {
                        0 => Vec::new(),
                        1 if index > 0 => vec![index - 1],
                        2 if index == 1 || index == 2 => vec![0],
                        2 if index == 3 => vec![1, 2],
                        _ => (0..index).filter(|prior| edges & (1 << prior) != 0).collect(),
                    };
                    Node { dependencies, importance, text }
                })
                .collect();
            if batch_insert && (shape == 1 || shape == 2) {
                nodes = reverse_ids(nodes);
            }
            let role = match role {
                0 => Role::Writer,
                1 => Role::Reviewer,
                2 => Role::Fixer,
                3 => Role::Evaluator,
                _ => Role::Evolver,
            };
            Case { role, nodes, batch_insert, operations }
        })
        .boxed()
}

fn node_id(index: usize) -> Result<ContextNodeId, String> {
    ContextNodeId::new(((index as u128) + 1).to_be_bytes()).map_err(|error| format!("{error:?}"))
}

fn reverse_ids(nodes: Vec<Node>) -> Vec<Node> {
    let count = nodes.len();
    nodes
        .into_iter()
        .rev()
        .map(|mut node| {
            node.dependencies =
                node.dependencies.into_iter().map(|index| count - 1 - index).collect();
            node.dependencies.sort_unstable();
            node
        })
        .collect()
}

fn limits() -> Result<WorkingLimits, String> {
    WorkingLimits::new(64, 64, 16_384, 64, 64).map_err(|error| error.to_string())
}

fn make_entry(index: usize, node: &Node, limits: WorkingLimits) -> Result<WorkingEntry, String> {
    let content_limits =
        ContextLimits::new(64, 16_384, 64, 5).map_err(|error| format!("{error:?}"))?;
    let bytes = node.text.as_bytes().to_vec();
    let content =
        bind_context_content(bytes.clone(), peritus_codec::sha256(&bytes), content_limits)
            .map_err(|error| format!("{error:?}"))?;
    let links = WorkingLinks::new(
        vec![ObservationId::new(1).map_err(|error| error.to_string())?],
        Vec::new(),
        node.dependencies.iter().map(|index| node_id(*index)).collect::<Result<_, _>>()?,
        limits,
    )
    .map_err(|error| error.to_string())?;
    let validity =
        WorkingValidity::new(None, None, Vec::new(), limits).map_err(|error| error.to_string())?;
    let (kind, status) = node.importance.production();
    WorkingEntry::new(node_id(index)?, kind, content, links, validity, limits)
        .and_then(|entry| entry.with_status(status))
        .map_err(|error| error.to_string())
}

fn build_state(case: &Case) -> Result<WorkingState, String> {
    let limits = limits()?;
    let binding = WorkingBinding::new(
        RunId::new([1; 16]).map_err(|error| format!("{error:?}"))?,
        WorkspaceId::new([2; 16]).map_err(|error| format!("{error:?}"))?,
        node_id(100)?,
        case.role.production(),
        1,
    );
    let environment =
        WorkingEnvironment::new(binding, Sha256Digest::new([3; 32]), Vec::new(), limits)
            .map_err(|error| error.to_string())?;
    let state = WorkingState::new(environment, limits).map_err(|error| error.to_string())?;
    let source = b"verified-local-fixture";
    let observation = ObservationSource::new(
        ObservationId::new(1).map_err(|error| error.to_string())?,
        peritus_codec::sha256(source),
        source.len() as u64,
        0,
        source.len() as u64,
        ObservationKind::ToolOutput,
    )
    .map_err(|error| error.to_string())?;
    let mut state = ingest_working_observation(&state, binding, observation)
        .map_err(|error| error.to_string())?;
    let entries = case
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| make_entry(index, node, limits))
        .collect::<Result<Vec<_>, _>>()?;
    if entries.is_empty() {
        return Ok(state);
    }
    if case.batch_insert {
        let delta = WorkingDelta::new(binding, state.revision(), entries, limits)
            .map_err(|error| error.to_string())?;
        state = apply_working_delta(&state, &delta).map_err(|error| error.to_string())?;
    } else {
        for entry in entries {
            let delta = WorkingDelta::new(binding, state.revision(), vec![entry], limits)
                .map_err(|error| error.to_string())?;
            state = apply_working_delta(&state, &delta).map_err(|error| error.to_string())?;
        }
    }
    Ok(state)
}

/// Independent breadth-first reference; never calls a production graph/closure validator.
fn closure(nodes: &[Node], roots: impl IntoIterator<Item = usize>) -> BTreeSet<usize> {
    let mut queue: VecDeque<_> = roots.into_iter().collect();
    let mut reached = BTreeSet::new();
    while let Some(index) = queue.pop_front() {
        if reached.insert(index) {
            queue.extend(nodes[index].dependencies.iter().copied());
        }
    }
    reached
}

fn bump(metrics: &mut crate::Metrics, name: &str, value: u64) {
    *metrics.entry(name.to_owned()).or_default() += value;
}

fn rendered_ids(view: &WorkingRenderView) -> Result<BTreeSet<usize>, String> {
    let mut result = BTreeSet::new();
    if let Some(plan) = view.plan() {
        for segment in plan.segments() {
            let raw = u128::from_be_bytes(*segment.source_id().as_bytes());
            let index = usize::try_from(raw.checked_sub(1).ok_or("zero rendered identity")?)
                .map_err(|error| error.to_string())?;
            if !result.insert(index) {
                return Err(format!("duplicate rendered entry {index}"));
            }
        }
    }
    Ok(result)
}

fn describe_render(result: &Result<WorkingRenderView, WorkingError>) -> String {
    match result {
        Ok(view) => format!("selected={:?}", rendered_ids(view)),
        Err(error) => format!("{error:?}"),
    }
}

fn complete_costs(
    case: &Case,
    state: &WorkingState,
    metrics: &mut crate::Metrics,
) -> Result<BTreeMap<usize, u64>, String> {
    let view = render_working_state_complete(state, state.binding())
        .map_err(|error| format!("complete working render failed: {error}"))?;
    let ids = rendered_ids(&view)?;
    let all: BTreeSet<_> = (0..case.nodes.len()).collect();
    let omitted: BTreeSet<_> = view
        .omitted()
        .iter()
        .map(|id| {
            usize::try_from(u128::from_be_bytes(*id.as_bytes()) - 1)
                .map_err(|error| error.to_string())
        })
        .collect::<Result<_, _>>()?;
    bump(metrics, "complete_renders", 1);
    if case.nodes.is_empty() {
        if view.plan().is_some() || !ids.is_empty() || !omitted.is_empty() {
            return Err("empty working state must produce no plan or omissions".into());
        }
        bump(metrics, "empty_state_renders", 1);
        return Ok(BTreeMap::new());
    }
    if case.role.excludes_memory() {
        if view.plan().is_some() || !ids.is_empty() || omitted != all {
            return Err("reviewer working memory must be entirely excluded".into());
        }
        bump(metrics, "excluded_role_renders", 1);
        return Ok(BTreeMap::new());
    }
    if ids != all || !omitted.is_empty() {
        return Err(format!("complete rendering lost entries: expected {all:?}, got {ids:?}"));
    }
    let plan = view.plan().ok_or("nonempty complete working state has no plan")?;
    let mut costs = BTreeMap::new();
    for segment in plan.segments() {
        let index = usize::try_from(u128::from_be_bytes(*segment.source_id().as_bytes()) - 1)
            .map_err(|error| error.to_string())?;
        if segment.authority() != AuthorityClass::NonAuthoritative
            || segment.message_role() != MessageRole::Evidence
            || segment.trust() != TrustClass::Untrusted
            || segment.provenance() != Provenance::Memory
        {
            return Err(format!("entry {index} gained instruction authority"));
        }
        let text = std::str::from_utf8(segment.content()).map_err(|error| error.to_string())?;
        if !text.contains(&format!("text={:?}\n", case.nodes[index].text)) {
            return Err(format!("entry {index} content lost or changed"));
        }
        // Public render bytes supply serialization cost, not membership or graph reachability.
        costs.insert(index, (segment.content().len() as u64).div_ceil(3));
    }
    let used: u64 = costs.values().sum();
    if used != plan.accounting().used_input() {
        return Err(format!("complete byte-derived token cost {used} disagrees with accounting"));
    }
    Ok(costs)
}

fn adjusted(base: u64, delta: i8) -> u64 {
    if delta < 0 {
        base.saturating_sub(u64::from(delta.unsigned_abs()))
    } else {
        base.saturating_add(delta as u64)
    }
}

fn probe(
    case: &Case,
    state: &WorkingState,
    operation: &Operation,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let Operation::Probe { basis, preferred_delta, available_delta, headroom } = operation else {
        return Err("probe called with non-probe operation".into());
    };
    let costs = complete_costs(case, state, metrics)?;
    let required = closure(
        &case.nodes,
        case.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.importance.required())
            .map(|(index, _)| index),
    );
    let needed: u64 = required.iter().map(|index| costs.get(index).copied().unwrap_or(0)).sum();
    let all_cost: u64 = costs.values().sum();
    let base = match basis {
        Basis::RequiredClosure => needed,
        Basis::AllEntries => all_cost,
    };
    let preferred = adjusted(base, *preferred_delta);
    let available = adjusted(base, *available_delta);
    let before = state.clone();
    let result = if *headroom {
        render_working_state_with_headroom(state, state.binding(), preferred, available)
    } else {
        render_working_state(state, state.binding(), preferred)
    };
    bump(metrics, "bounded_renders", 1);
    if *headroom {
        bump(metrics, "headroom_renders", 1);
    }
    if *preferred_delta == -1 {
        bump(metrics, "below_boundary_probes", 1);
    }
    if *preferred_delta == 0 {
        bump(metrics, "exact_boundary_probes", 1);
    }
    if *preferred_delta == 1 {
        bump(metrics, "above_boundary_probes", 1);
    }
    if state != &before {
        return Err("render mutated retained state".into());
    }
    if case.nodes.is_empty() {
        let view = result.map_err(|error| format!("empty working render failed: {error}"))?;
        if view.plan().is_some() || !view.omitted().is_empty() {
            return Err(
                "empty bounded state must produce no plan or omissions, even at zero capacity"
                    .into(),
            );
        }
        bump(metrics, "empty_bounded_renders", 1);
        return Ok(());
    }
    if case.role.excludes_memory() {
        let view = result.map_err(|error| error.to_string())?;
        if view.plan().is_some() || view.omitted().len() != case.nodes.len() {
            return Err("bounded rendering leaked reviewer memory".into());
        }
        return Ok(());
    }
    let initial = if *headroom { preferred.min(available) } else { preferred };
    let ceiling = if *headroom { available } else { preferred };
    if initial == 0 {
        match result {
            Err(WorkingError::Projection(error))
                if error.kind() == ContextErrorKind::InvalidTokenBudget =>
            {
                bump(metrics, "zero_budget_rejections", 1);
                return Ok(());
            }
            other => {
                return Err(format!(
                    "zero working allocation expected InvalidTokenBudget, got {}",
                    describe_render(&other)
                ));
            }
        }
    }
    if needed > ceiling {
        match result {
            Err(WorkingError::Projection(error))
                if error.kind() == ContextErrorKind::RequiredTokenBudgetExceeded =>
            {
                bump(metrics, "required_capacity_rejections", 1);
                return Ok(());
            }
            other => {
                return Err(format!(
                    "required closure costs {needed}, ceiling {ceiling}; got {}",
                    describe_render(&other)
                ));
            }
        }
    }
    let effective = initial.max(needed);
    let mut selected = required;
    let mut used = needed;
    // Every working optional root has identical authority/priority/recency. Stable ID is the
    // documented tiebreak; this simple set model uses no production ordering/closure helpers.
    for index in 0..case.nodes.len() {
        if selected.contains(&index) {
            continue;
        }
        let candidate = closure(&case.nodes, [index]);
        let newly_needed: u64 = candidate.difference(&selected).map(|index| costs[index]).sum();
        if used + newly_needed <= effective {
            selected.extend(candidate);
            used += newly_needed;
        }
    }
    let view = result.map_err(|error| format!("feasible closure render failed: {error}"))?;
    let actual = rendered_ids(&view)?;
    if actual != selected {
        return Err(format!(
            "working closure mismatch: expected {selected:?}, got {actual:?}; limit={effective}"
        ));
    }
    let omitted: BTreeSet<_> =
        view.omitted().iter().map(|id| u128::from_be_bytes(*id.as_bytes()) - 1).collect();
    let expected_omitted: BTreeSet<_> = (0..case.nodes.len())
        .filter(|index| !selected.contains(index))
        .map(|index| index as u128)
        .collect();
    if omitted != expected_omitted {
        return Err("working omissions are not the exact complement".into());
    }
    let plan = view.plan().ok_or("nonempty state returned no selected render plan")?;
    if plan.accounting().used_input() != used || used > effective {
        return Err("bounded rendering token accounting differs from selected bytes".into());
    }
    if closure(&case.nodes, selected.iter().copied()) != selected {
        return Err("selected working entries omit a prerequisite".into());
    }
    bump(metrics, "successful_bounded_renders", 1);
    bump(metrics, "selected_entries", actual.len() as u64);
    bump(metrics, "omitted_entries", omitted.len() as u64);
    Ok(())
}

/// Executes real reducers/renderers against the independent case-derived closure model.
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.nodes.len() > 32 || case.operations.len() > 1024 {
        return Err("case exceeds discovery limits".into());
    }
    if case.nodes.is_empty()
        && case.operations.iter().any(|operation| !matches!(operation, Operation::Probe { .. }))
    {
        return Err("empty-state cases support only explicit render probes".into());
    }
    for (index, node) in case.nodes.iter().enumerate() {
        if node.text.is_empty()
            || node.text.len() > 4096
            || node.dependencies.iter().any(|dependency| {
                *dependency >= case.nodes.len() || (!case.batch_insert && *dependency >= index)
            })
            || node.dependencies.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(format!("case node {index} is not a bounded canonical DAG input"));
        }
    }
    for (index, node) in case.nodes.iter().enumerate() {
        if closure(&case.nodes, node.dependencies.iter().copied()).contains(&index) {
            return Err(format!("case node {index} participates in a cycle"));
        }
    }
    let mut metrics = crate::Metrics::new();
    let mut current = case.clone();
    let mut state = build_state(&current)?;
    bump(&mut metrics, "cases", 1);
    bump(&mut metrics, "initial_entries", current.nodes.len() as u64);
    bump(
        &mut metrics,
        "dependency_edges",
        current.nodes.iter().map(|node| node.dependencies.len() as u64).sum(),
    );
    bump(
        &mut metrics,
        "forward_dependency_edges",
        current
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                node.dependencies.iter().filter(|dependency| **dependency > index).count() as u64
            })
            .sum(),
    );
    bump(
        &mut metrics,
        "required_roots",
        current.nodes.iter().filter(|node| node.importance.required()).count() as u64,
    );
    let mut references = BTreeMap::<usize, usize>::new();
    for dependency in current.nodes.iter().flat_map(|node| &node.dependencies) {
        *references.entry(*dependency).or_default() += 1;
    }
    bump(
        &mut metrics,
        "shared_prerequisites",
        references.values().filter(|count| **count > 1).count() as u64,
    );
    bump(
        &mut metrics,
        if current.batch_insert { "batch_histories" } else { "incremental_histories" },
        1,
    );
    for delta in [-1, 0, 1] {
        probe(
            &current,
            &state,
            &Operation::Probe {
                basis: Basis::RequiredClosure,
                preferred_delta: delta,
                available_delta: 1,
                headroom: false,
            },
            &mut metrics,
        )?;
    }
    for (step, operation) in case.operations.iter().enumerate() {
        let result = match operation {
            Operation::Probe { .. } => probe(&current, &state, operation, &mut metrics),
            Operation::Replace { index, importance, text } => {
                if text.is_empty() || text.len() > 4096 {
                    return Err("invalid replacement text".into());
                }
                let index = *index % current.nodes.len();
                let old_length = current.nodes[index].text.len();
                let content_changed = current.nodes[index].text != *text;
                let mut replacement = current.nodes[index].clone();
                replacement.importance = *importance;
                replacement.text.clone_from(text);
                let entry = make_entry(index, &replacement, limits()?)?;
                let delta =
                    WorkingDelta::new(state.binding(), state.revision(), vec![entry], limits()?)
                        .map_err(|error| error.to_string())?;
                state = apply_working_delta(&state, &delta).map_err(|error| error.to_string())?;
                current.nodes[index] = replacement;
                bump(&mut metrics, "successful_replacements", 1);
                if content_changed && old_length == text.len() {
                    bump(&mut metrics, "same_length_replacements", 1);
                }
                complete_costs(&current, &state, &mut metrics).map(|_| ())
            }
            Operation::RejectRevision
            | Operation::RejectForeignBinding
            | Operation::RejectDependencyCycle => {
                let before = state.clone();
                let mut binding = state.binding();
                let mut revision = state.revision();
                let mut node = current.nodes[0].clone();
                let expected = match operation {
                    Operation::RejectRevision => {
                        revision = revision.saturating_sub(1);
                        bump(&mut metrics, "stale_revision_attempts", 1);
                        WorkingError::RevisionMismatch
                    }
                    Operation::RejectForeignBinding => {
                        binding = WorkingBinding::new(
                            binding.run(),
                            binding.workspace(),
                            node_id(101)?,
                            binding.role(),
                            binding.conversation_revision(),
                        );
                        bump(&mut metrics, "foreign_scope_attempts", 1);
                        WorkingError::BindingMismatch
                    }
                    _ => {
                        node.dependencies = vec![0];
                        bump(&mut metrics, "cycle_attempts", 1);
                        WorkingError::DependencyCycle
                    }
                };
                let entry = make_entry(0, &node, limits()?)?;
                let delta = WorkingDelta::new(binding, revision, vec![entry], limits()?)
                    .map_err(|error| error.to_string())?;
                match apply_working_delta(&state, &delta) {
                    Err(error) if error == expected => {}
                    other => return Err(format!("expected rejection {expected:?}, got {other:?}")),
                }
                if state != before {
                    return Err("rejected delta changed retained state".into());
                }
                bump(&mut metrics, "continued_after_rejection", 1);
                complete_costs(&current, &state, &mut metrics).map(|_| ())
            }
        };
        result.map_err(|error| format!("step {step}, {operation:?}: {error}"))?;
    }
    // Re-render all boundaries after the history, including mutations and rejected updates.
    for headroom in [false, true] {
        probe(
            &current,
            &state,
            &Operation::Probe {
                basis: Basis::AllEntries,
                preferred_delta: -1,
                available_delta: 1,
                headroom,
            },
            &mut metrics,
        )?;
    }
    Ok(metrics)
}

/// An optional early-ID distractor must not displace the later required plan's closure.
pub fn calibration_case() -> Case {
    Case {
        role: Role::Writer,
        nodes: vec![
            Node {
                dependencies: vec![],
                importance: Importance::Optional,
                text: "distractor".into(),
            },
            Node {
                dependencies: vec![],
                importance: Importance::Optional,
                text: "prerequisite".into(),
            },
            Node {
                dependencies: vec![1],
                importance: Importance::OpenPlan,
                text: "required-plan".into(),
            },
        ],
        batch_insert: true,
        operations: vec![Operation::RejectRevision, Operation::RejectDependencyCycle],
    }
}

/// Stable witnesses cover calibration, a diamond, long chains, and excluded reviewer memory.
pub fn examples() -> Vec<Case> {
    let mut diamond = calibration_case();
    diamond.nodes = vec![
        Node { dependencies: vec![], importance: Importance::Optional, text: "é🦀".into() },
        Node { dependencies: vec![0], importance: Importance::Optional, text: "left".into() },
        Node { dependencies: vec![0], importance: Importance::Optional, text: "right".into() },
        Node {
            dependencies: vec![1, 2],
            importance: Importance::FailedApproach,
            text: "join".into(),
        },
    ];
    diamond.batch_insert = false;
    diamond.operations.push(Operation::Probe {
        basis: Basis::RequiredClosure,
        preferred_delta: -1,
        available_delta: 0,
        headroom: true,
    });
    let mut chain = calibration_case();
    chain.nodes = (0..12)
        .map(|index| Node {
            dependencies: if index == 0 { Vec::new() } else { vec![index - 1] },
            importance: if index == 11 { Importance::Contradicted } else { Importance::Optional },
            text: format!("chain-{index}"),
        })
        .collect();
    let mut reviewer = diamond.clone();
    reviewer.role = Role::Reviewer;
    reviewer.operations.push(Operation::RejectForeignBinding);
    let mut forward_diamond = diamond.clone();
    forward_diamond.batch_insert = true;
    forward_diamond.nodes = reverse_ids(forward_diamond.nodes);
    let mut replacement = calibration_case();
    replacement.nodes[0].text = "left".into();
    replacement.operations.push(Operation::Replace {
        index: 0,
        importance: Importance::Optional,
        text: "rght".into(),
    });
    let empty = Case {
        role: Role::Writer,
        nodes: Vec::new(),
        batch_insert: true,
        operations: vec![Operation::Probe {
            basis: Basis::AllEntries,
            preferred_delta: 0,
            available_delta: 0,
            headroom: true,
        }],
    };
    let mut empty_reviewer = empty.clone();
    empty_reviewer.role = Role::Reviewer;
    vec![
        calibration_case(),
        diamond,
        chain,
        reviewer,
        forward_diamond,
        replacement,
        empty,
        empty_reviewer,
    ]
}
