//! Stateful working-memory invalidation against independent reverse-graph reachability.

use peritus_context::working::{
    ObservationId, ObservationKind, ObservationSource, WorkingBinding, WorkingDelta, WorkingEntry,
    WorkingEntryKind, WorkingEntryStatus, WorkingEnvironment, WorkingError, WorkingEvent,
    WorkingFileDigest, WorkingHostBinding, WorkingLimits, WorkingLinks, WorkingState,
    WorkingValidity, apply_working_event, decode_working_state, encode_working_state,
    ingest_working_observation, refresh_working_state, render_working_state_complete,
};
use peritus_context::{AuthorityClass, ContextLimits, ContextNodeId, bind_context_content};
use peritus_role::HarnessRole;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub role: u8,
    pub nodes: Vec<Node>,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub dependencies: Vec<usize>,
    pub file: Option<u8>,
    pub candidate: bool,
    pub conversation: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    File { index: u8, value: Option<u8> },
    Candidate { value: u8 },
    Conversation,
    Ingest { value: u8 },
    Repair { index: u8, value: u8 },
    HostInvalidate { index: u8 },
    RejectUnprovenRepair { index: u8 },
    RejectScope { dimension: u8 },
    RejectRevision,
    RejectConversation,
    RejectSourceReuse,
    Reopen,
    Read,
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        5 => (0_u8..4, prop::option::of(any::<u8>())).prop_map(|(index, value)| Operation::File { index, value }),
        2 => any::<u8>().prop_map(|value| Operation::Candidate { value }),
        2 => Just(Operation::Conversation),
        2 => any::<u8>().prop_map(|value| Operation::Ingest { value }),
        4 => (any::<u8>(), any::<u8>()).prop_map(|(index, value)| Operation::Repair { index, value }),
        2 => any::<u8>().prop_map(|index| Operation::HostInvalidate { index }),
        2 => any::<u8>().prop_map(|index| Operation::RejectUnprovenRepair { index }),
        1 => (0_u8..4).prop_map(|dimension| Operation::RejectScope { dimension }),
        1 => Just(Operation::RejectRevision),
        1 => Just(Operation::RejectConversation),
        1 => Just(Operation::RejectSourceReuse),
        2 => Just(Operation::Reopen),
        2 => Just(Operation::Read),
    ];
    (
        0_u8..5,
        any::<bool>(),
        prop::collection::vec(
            (any::<u16>(), prop::option::of(0_u8..4), any::<bool>(), any::<bool>()),
            1..=max_steps.clamp(3, 12),
        ),
        prop::collection::vec(operation, 0..=max_steps.min(256)),
    )
        .prop_map(|(role, reverse, raw, operations)| {
            let count = raw.len();
            let mut nodes: Vec<_> = raw
                .into_iter()
                .enumerate()
                .map(|(index, (edges, file, candidate, conversation))| Node {
                    dependencies: (0..index)
                        .filter(|prior| edges & (1_u16 << *prior) != 0)
                        .collect(),
                    file,
                    candidate,
                    conversation,
                })
                .collect();
            if reverse {
                nodes = nodes
                    .into_iter()
                    .rev()
                    .map(|mut node| {
                        node.dependencies =
                            node.dependencies.into_iter().map(|index| count - 1 - index).collect();
                        node.dependencies.sort_unstable();
                        node
                    })
                    .collect();
            }
            Case { role, nodes, operations }
        })
        .boxed()
}

#[derive(Clone)]
struct Capture {
    file: Option<(usize, u8)>,
    candidate: Option<u8>,
    conversation: Option<u64>,
    unknown: bool,
}

#[derive(Clone)]
struct Entry {
    capture: Capture,
    stale: bool,
    stale_through: u64,
    value: u8,
    source: u64,
}

struct Model {
    files: [Option<u8>; 4],
    candidate: u8,
    conversation: u64,
    revision: u64,
    observations: u64,
    entries: Vec<Entry>,
}

fn digest(value: u8) -> Sha256Digest {
    peritus_codec::sha256(&[value; 8])
}
fn id(index: usize) -> Result<ContextNodeId, String> {
    ContextNodeId::new(((index as u128) + 1).to_be_bytes()).map_err(|error| format!("{error:?}"))
}
fn role(value: u8) -> HarnessRole {
    match value % 5 {
        0 => HarnessRole::Writer,
        1 => HarnessRole::Reviewer,
        2 => HarnessRole::Fixer,
        3 => HarnessRole::Evaluator,
        _ => HarnessRole::Evolver,
    }
}
fn limits() -> Result<WorkingLimits, String> {
    WorkingLimits::new(1024, 64, 4096, 64, 64).map_err(|error| error.to_string())
}
fn binding(case: &Case, conversation: u64) -> Result<WorkingBinding, String> {
    Ok(WorkingBinding::new(
        RunId::new([1; 16]).map_err(|error| format!("{error:?}"))?,
        WorkspaceId::new([2; 16]).map_err(|error| format!("{error:?}"))?,
        id(100)?,
        role(case.role),
        conversation,
    ))
}
fn environment(case: &Case, model: &Model) -> Result<WorkingEnvironment, String> {
    let files = model
        .files
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.map(|value| (index, value)))
        .map(|(index, value)| Ok(WorkingFileDigest::new(id(200 + index)?, digest(value))))
        .collect::<Result<Vec<_>, String>>()?;
    WorkingEnvironment::new(
        binding(case, model.conversation)?,
        digest(model.candidate),
        files,
        limits()?,
    )
    .map_err(|error| error.to_string())
}
fn capture(node: &Node, model: &Model) -> Capture {
    Capture {
        file: node.file.map(|index| {
            let index = usize::from(index) % 4;
            (index, model.files[index].unwrap_or(0))
        }),
        candidate: node.candidate.then_some(model.candidate),
        conversation: node.conversation.then_some(model.conversation),
        unknown: false,
    }
}
fn validity(capture: &Capture) -> Result<WorkingValidity, String> {
    if capture.unknown {
        return Ok(WorkingValidity::uncertain());
    }
    WorkingValidity::new(
        capture.conversation,
        capture.candidate.map(digest),
        capture
            .file
            .map(|(index, value)| Ok(WorkingFileDigest::new(id(200 + index)?, digest(value))))
            .into_iter()
            .collect::<Result<Vec<_>, String>>()?,
        limits()?,
    )
    .map_err(|error| error.to_string())
}
fn holds(capture: &Capture, model: &Model) -> bool {
    !capture.unknown
        && capture.file.is_none_or(|(index, value)| model.files[index] == Some(value))
        && capture.candidate.is_none_or(|value| value == model.candidate)
        && capture.conversation.is_none_or(|value| value == model.conversation)
}

/// Full recomputation with a queue of invalid roots and reverse adjacency, not production passes.
fn recompute(case: &Case, model: &mut Model, metrics: &mut crate::Metrics) {
    let previous: Vec<_> = model.entries.iter().map(|entry| entry.stale).collect();
    let direct: Vec<_> = model.entries.iter().map(|entry| !holds(&entry.capture, model)).collect();
    let mut reverse = vec![Vec::new(); case.nodes.len()];
    for (index, node) in case.nodes.iter().enumerate() {
        for dependency in &node.dependencies {
            reverse[*dependency].push(index);
        }
    }
    let mut stale = BTreeSet::new();
    let mut queue: VecDeque<_> =
        (0..case.nodes.len()).filter(|index| previous[*index] || direct[*index]).collect();
    while let Some(index) = queue.pop_front() {
        if stale.insert(index) {
            queue.extend(reverse[index].iter().copied());
        }
    }
    for (index, entry) in model.entries.iter_mut().enumerate() {
        if stale.contains(&index) {
            if !previous[index] {
                bump(
                    metrics,
                    if direct[index] { "direct_invalidations" } else { "transitive_invalidations" },
                    1,
                );
            }
            if !previous[index] || direct[index] {
                entry.stale_through = model.observations;
            }
            entry.stale = true;
        }
    }
}
fn bump(metrics: &mut crate::Metrics, key: &str, value: u64) {
    *metrics.entry(key.into()).or_default() += value;
}
fn source(sequence: u64, value: u8) -> Result<ObservationSource, String> {
    ObservationSource::new(
        ObservationId::new(sequence).map_err(|error| error.to_string())?,
        digest(value),
        8,
        0,
        8,
        ObservationKind::ToolOutput,
    )
    .map_err(|error| error.to_string())
}
fn production_entry(case: &Case, index: usize, entry: &Entry) -> Result<WorkingEntry, String> {
    let bytes = format!("value:{:03}", entry.value).into_bytes();
    let content = bind_context_content(
        bytes.clone(),
        peritus_codec::sha256(&bytes),
        ContextLimits::new(64, 4096, 64, 5).map_err(|error| format!("{error:?}"))?,
    )
    .map_err(|error| format!("{error:?}"))?;
    let links = WorkingLinks::new(
        vec![ObservationId::new(entry.source).map_err(|error| error.to_string())?],
        Vec::new(),
        case.nodes[index].dependencies.iter().map(|index| id(*index)).collect::<Result<_, _>>()?,
        limits()?,
    )
    .map_err(|error| error.to_string())?;
    WorkingEntry::new(
        id(index)?,
        WorkingEntryKind::Assertion,
        content,
        links,
        validity(&entry.capture)?,
        limits()?,
    )
    .map_err(|error| error.to_string())
}
fn host_event(state: &WorkingState, entries: Vec<WorkingEntry>) -> Result<WorkingEvent, String> {
    Ok(WorkingEvent::HostDelta {
        delta: WorkingDelta::new(state.binding(), state.revision(), entries, limits()?)
            .map_err(|error| error.to_string())?,
        dependencies: WorkingValidity::new(None, None, Vec::new(), limits()?)
            .map_err(|error| error.to_string())?,
        source_binding: WorkingHostBinding::new(state.through_observation(), Some(digest(90))),
    })
}
fn compare(
    case: &Case,
    model: &Model,
    state: &WorkingState,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    if state.binding() != binding(case, model.conversation)?
        || state.revision() != model.revision
        || state.through_observation() != model.observations
    {
        return Err(format!(
            "binding/revision/source frontier mismatch actual {}/{}, expected {}/{}",
            state.revision(),
            state.through_observation(),
            model.revision,
            model.observations
        ));
    }
    let entries = state.entries(state.binding()).map_err(|error| error.to_string())?;
    if entries.len() != model.entries.len() {
        return Err("retained entry count changed".into());
    }
    for (index, (actual, expected)) in entries.iter().zip(&model.entries).enumerate() {
        let status =
            if expected.stale { WorkingEntryStatus::Stale } else { WorkingEntryStatus::Open };
        if actual.id() != id(index)?
            || actual.status() != status
            || actual.stale_through() != expected.stale_through
            || actual.content().bytes() != format!("value:{:03}", expected.value).as_bytes()
            || actual.authority() != AuthorityClass::NonAuthoritative
            || actual.links().supports()
                != [ObservationId::new(expected.source).map_err(|error| error.to_string())?]
        {
            return Err(format!(
                "entry {index}: status {:?}/frontier {} vs {status:?}/{}",
                actual.status(),
                actual.stale_through(),
                expected.stale_through
            ));
        }
    }
    let view =
        render_working_state_complete(state, state.binding()).map_err(|error| error.to_string())?;
    if case.role % 5 == 1 {
        if view.plan().is_some() || view.omitted().len() != case.nodes.len() {
            return Err("reviewer role leaked working state".into());
        }
        bump(metrics, "role_excluded_reads", 1);
    } else if view.plan().is_none_or(|plan| plan.segments().len() != case.nodes.len()) {
        return Err("complete read lost stale retained records".into());
    }
    bump(metrics, "state_comparisons", 1);
    Ok(())
}

fn invalidate(
    case: &Case,
    model: &mut Model,
    state: &mut WorkingState,
    index: usize,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let event = WorkingEvent::HostInvalidate {
        binding: state.binding(),
        base_revision: state.revision(),
        entries: vec![id(index)?],
    };
    *state = apply_working_event(state, &event).map_err(|error| error.to_string())?;
    model.entries[index].capture =
        Capture { file: None, candidate: None, conversation: None, unknown: true };
    model.revision += 1;
    recompute(case, model, metrics);
    bump(metrics, "explicit_host_invalidations", 1);
    Ok(())
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.role >= 5
        || case.nodes.is_empty()
        || case.nodes.len() > 32
        || case.operations.len() > 512
    {
        return Err("case exceeds domain discovery bounds".into());
    }
    for (index, node) in case.nodes.iter().enumerate() {
        if node.file.is_some_and(|key| key >= 4)
            || node
                .dependencies
                .iter()
                .any(|target| *target >= case.nodes.len() || *target == index)
            || node.dependencies.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err("invalid case graph".into());
        }
    }
    for root in 0..case.nodes.len() {
        let mut seen = BTreeSet::new();
        let mut pending = case.nodes[root].dependencies.clone();
        while let Some(index) = pending.pop() {
            if index == root {
                return Err("cyclic case graph".into());
            }
            if seen.insert(index) {
                pending.extend(&case.nodes[index].dependencies);
            }
        }
    }
    let mut metrics = crate::Metrics::new();
    let mut model = Model {
        files: [Some(1), Some(2), Some(3), Some(4)],
        candidate: 1,
        conversation: 1,
        revision: 0,
        observations: 0,
        entries: Vec::new(),
    };
    let mut state = WorkingState::new(environment(case, &model)?, limits()?)
        .map_err(|error| error.to_string())?;
    state = ingest_working_observation(&state, state.binding(), source(1, 1)?)
        .map_err(|error| error.to_string())?;
    model.revision = 1;
    model.observations = 1;
    model.entries = case
        .nodes
        .iter()
        .map(|node| Entry {
            capture: capture(node, &model),
            stale: false,
            stale_through: 0,
            value: 1,
            source: 1,
        })
        .collect();
    let entries = model
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| production_entry(case, index, entry))
        .collect::<Result<_, _>>()?;
    state = apply_working_event(&state, &host_event(&state, entries)?)
        .map_err(|error| error.to_string())?;
    model.revision += 1;
    compare(case, &model, &state, &mut metrics)?;
    for (step, operation) in case.operations.iter().enumerate() {
        let result = (|| {
            match *operation {
                Operation::File { index, value } => {
                    let index = usize::from(index) % 4;
                    let changed = model.files[index] != value;
                    model.files[index] = value;
                    state =
                        refresh_working_state(&state, state.revision(), environment(case, &model)?)
                            .map_err(|error| error.to_string())?;
                    if changed {
                        model.revision += 1;
                        recompute(case, &mut model, &mut metrics);
                        bump(&mut metrics, "file_changes", 1);
                        bump(&mut metrics, "same_size_source_changes", u64::from(value.is_some()));
                    } else {
                        bump(&mut metrics, "same_environment_refreshes", 1);
                    }
                }
                Operation::Candidate { value } => {
                    let changed = model.candidate != value;
                    model.candidate = value;
                    state =
                        refresh_working_state(&state, state.revision(), environment(case, &model)?)
                            .map_err(|error| error.to_string())?;
                    if changed {
                        model.revision += 1;
                        recompute(case, &mut model, &mut metrics);
                        bump(&mut metrics, "candidate_changes", 1);
                    }
                }
                Operation::Conversation => {
                    model.conversation += 1;
                    state =
                        refresh_working_state(&state, state.revision(), environment(case, &model)?)
                            .map_err(|error| error.to_string())?;
                    model.revision += 1;
                    recompute(case, &mut model, &mut metrics);
                    bump(&mut metrics, "conversation_changes", 1);
                }
                Operation::Ingest { value } => {
                    model.observations += 1;
                    state = ingest_working_observation(
                        &state,
                        state.binding(),
                        source(model.observations, value)?,
                    )
                    .map_err(|error| error.to_string())?;
                    model.revision += 1;
                    bump(&mut metrics, "new_observations", 1);
                }
                Operation::Repair { index, value } => {
                    let index = usize::from(index) % model.entries.len();
                    model.observations += 1;
                    state = ingest_working_observation(
                        &state,
                        state.binding(),
                        source(model.observations, value)?,
                    )
                    .map_err(|error| error.to_string())?;
                    model.revision += 1;
                    let old_value = model.entries[index].value;
                    let was_stale = model.entries[index].stale;
                    model.entries[index] = Entry {
                        capture: capture(&case.nodes[index], &model),
                        stale: false,
                        stale_through: 0,
                        value,
                        source: model.observations,
                    };
                    let entry = production_entry(case, index, &model.entries[index])?;
                    state = apply_working_event(&state, &host_event(&state, vec![entry])?)
                        .map_err(|error| error.to_string())?;
                    model.revision += 1;
                    recompute(case, &mut model, &mut metrics);
                    bump(&mut metrics, "fresh_source_repairs", 1);
                    bump(&mut metrics, "same_size_content_changes", u64::from(value != old_value));
                    if was_stale && !model.entries[index].stale {
                        bump(&mut metrics, "restored_current_entries", 1);
                    }
                }
                Operation::HostInvalidate { index } => {
                    let index = usize::from(index) % model.entries.len();
                    invalidate(case, &mut model, &mut state, index, &mut metrics)?;
                }
                Operation::RejectUnprovenRepair { index } => {
                    let index = usize::from(index) % model.entries.len();
                    invalidate(case, &mut model, &mut state, index, &mut metrics)?;
                    let mut entry = model.entries[index].clone();
                    entry.capture = capture(&case.nodes[index], &model);
                    entry.stale = false;
                    let event = host_event(&state, vec![production_entry(case, index, &entry)?])?;
                    if !matches!(apply_working_event(&state, &event), Err(WorkingError::StaleEntry))
                    {
                        return Err("stale entry accepted without post-frontier source".into());
                    }
                    bump(&mut metrics, "stale_source_rejections", 1);
                }
                Operation::RejectScope { dimension } => {
                    let original = state.binding();
                    let foreign = WorkingBinding::new(
                        if dimension % 4 == 0 {
                            RunId::new([9; 16]).map_err(|error| format!("{error:?}"))?
                        } else {
                            original.run()
                        },
                        if dimension % 4 == 1 {
                            WorkspaceId::new([9; 16]).map_err(|error| format!("{error:?}"))?
                        } else {
                            original.workspace()
                        },
                        if dimension % 4 == 2 { id(101)? } else { original.task() },
                        if dimension % 4 == 3 { role(case.role + 1) } else { original.role() },
                        original.conversation_revision(),
                    );
                    let env = WorkingEnvironment::new(
                        foreign,
                        state.environment().candidate(),
                        state.environment().files().to_vec(),
                        limits()?,
                    )
                    .map_err(|error| error.to_string())?;
                    if !matches!(
                        refresh_working_state(&state, state.revision(), env),
                        Err(WorkingError::BindingMismatch)
                    ) || !matches!(state.entries(foreign), Err(WorkingError::BindingMismatch))
                    {
                        return Err("foreign scope was not rejected".into());
                    }
                    bump(&mut metrics, "scope_rejections", 1);
                }
                Operation::RejectRevision => {
                    if !matches!(
                        refresh_working_state(
                            &state,
                            state.revision() - 1,
                            environment(case, &model)?
                        ),
                        Err(WorkingError::RevisionMismatch)
                    ) {
                        return Err("stale revision was accepted".into());
                    }
                    bump(&mut metrics, "revision_rejections", 1);
                }
                Operation::RejectConversation => {
                    let mut env_model = Model {
                        files: model.files,
                        candidate: model.candidate,
                        conversation: model.conversation - 1,
                        revision: 0,
                        observations: 0,
                        entries: Vec::new(),
                    };
                    env_model.entries.clear();
                    if !matches!(
                        refresh_working_state(
                            &state,
                            state.revision(),
                            environment(case, &env_model)?
                        ),
                        Err(WorkingError::StaleConversation)
                    ) {
                        return Err("backwards conversation was accepted".into());
                    }
                    bump(&mut metrics, "conversation_rejections", 1);
                }
                Operation::RejectSourceReuse => {
                    if !matches!(
                        ingest_working_observation(&state, state.binding(), source(1, 2)?),
                        Err(WorkingError::SourceConflict)
                    ) {
                        return Err("conflicting observation identity was accepted".into());
                    }
                    bump(&mut metrics, "source_conflict_rejections", 1);
                }
                Operation::Reopen => {
                    let bytes = encode_working_state(&state).map_err(|error| error.to_string())?;
                    let restored = decode_working_state(&bytes, state.binding(), limits()?)
                        .map_err(|error| error.to_string())?;
                    if restored != state {
                        return Err("working snapshot changed during reopen".into());
                    }
                    state = restored;
                    bump(&mut metrics, "wire_reopens", 1);
                }
                Operation::Read => bump(&mut metrics, "explicit_reads", 1),
            }
            compare(case, &model, &state, &mut metrics)
        })();
        result.map_err(|error| format!("step {step} {operation:?}: {error}"))?;
        bump(&mut metrics, "operations", 1);
    }
    bump(&mut metrics, "cases", 1);
    Ok(metrics)
}

pub fn calibration_case() -> Case {
    Case {
        role: 0,
        nodes: vec![
            Node { dependencies: vec![1], file: None, candidate: false, conversation: false },
            Node { dependencies: vec![2], file: None, candidate: false, conversation: false },
            Node { dependencies: vec![], file: Some(0), candidate: false, conversation: false },
        ],
        operations: vec![
            Operation::Read,
            Operation::File { index: 0, value: Some(9) },
            Operation::Reopen,
        ],
    }
}

pub fn examples() -> Vec<Case> {
    let mut broad = calibration_case();
    broad.nodes.push(Node {
        dependencies: vec![],
        file: Some(1),
        candidate: false,
        conversation: false,
    });
    broad.operations.extend([
        Operation::Ingest { value: 3 },
        Operation::File { index: 0, value: Some(1) },
        Operation::Repair { index: 2, value: 2 },
        Operation::Repair { index: 1, value: 3 },
        Operation::Repair { index: 0, value: 4 },
        Operation::RejectUnprovenRepair { index: 1 },
        Operation::Reopen,
        Operation::RejectRevision,
        Operation::RejectConversation,
        Operation::RejectSourceReuse,
    ]);
    for dimension in 0..4 {
        broad.operations.push(Operation::RejectScope { dimension });
    }
    let mut axes = Case {
        role: 2,
        nodes: vec![
            Node { dependencies: vec![], file: None, candidate: true, conversation: false },
            Node { dependencies: vec![], file: None, candidate: false, conversation: true },
        ],
        operations: vec![
            Operation::Candidate { value: 8 },
            Operation::Conversation,
            Operation::Reopen,
        ],
    };
    axes.operations.extend([
        Operation::Repair { index: 0, value: 8 },
        Operation::Repair { index: 1, value: 9 },
        Operation::File { index: 0, value: None },
        Operation::Read,
    ]);
    let mut reviewer = broad.clone();
    reviewer.role = 1;
    vec![calibration_case(), broad, axes, reviewer]
}
