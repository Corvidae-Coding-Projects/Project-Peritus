//! Source/lineage invalidation and delta delivery against independent maps and graph closure.

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CandidateIdentity, CurrentKnowledgeState, DeltaDelivery, InvalidationReason,
    InvalidationRequest, KnowledgeBinding, KnowledgeChange, KnowledgeErrorKind, KnowledgeLimits,
    KnowledgeSection, KnowledgeSectionId, KnowledgeSectionKind, KnowledgeSourceId, ReuseDecision,
    RunKnowledgeSnapshot, SourceDigest, plan_delta_packet, plan_invalidation,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub role: u8,
    pub sections: Vec<Section>,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub kind: Kind,
    pub sources: Vec<usize>,
    pub dependencies: Vec<usize>,
    pub material: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Inventory,
    FileMap,
    Requirements,
    Design,
    Compacted,
    Finding,
    Evidence,
    Navigation,
}
impl Kind {
    fn production(self) -> KnowledgeSectionKind {
        match self {
            Self::Inventory => KnowledgeSectionKind::RepositoryInventory,
            Self::FileMap => KnowledgeSectionKind::RelevantFileMap,
            Self::Requirements => KnowledgeSectionKind::LiteralRequirementLedger,
            Self::Design => KnowledgeSectionKind::DesignSection,
            Self::Compacted => KnowledgeSectionKind::CompactedToolObservation,
            Self::Finding => KnowledgeSectionKind::ResolvedFinding,
            Self::Evidence => KnowledgeSectionKind::CandidateEvidenceIndex,
            Self::Navigation => KnowledgeSectionKind::NavigationSummary,
        }
    }
    fn navigation(self) -> bool {
        matches!(self, Self::Compacted | Self::Navigation)
    }
    fn conversation(self) -> bool {
        matches!(self, Self::Requirements | Self::Design | Self::Navigation)
    }
    fn candidate(self) -> bool {
        matches!(self, Self::Compacted | Self::Finding | Self::Evidence | Self::Navigation)
    }
    fn clarification(self) -> bool {
        matches!(self, Self::Requirements | Self::Design)
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Axis {
    Run,
    Workspace,
    Content,
    Repository,
    Execution,
    Conversation,
    Checkpoint,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Source { index: u8, value: Option<u8> },
    Axis { axis: Axis, value: u8 },
    Clarify { index: u8 },
    ProviderFailure,
    Capture { index: u8, material: u8 },
    RejectRole,
    RejectCurrent,
    Reconstruct,
    Inspect,
}
fn kind_strategy() -> BoxedStrategy<Kind> {
    prop_oneof![
        Just(Kind::Inventory),
        Just(Kind::FileMap),
        Just(Kind::Requirements),
        Just(Kind::Design),
        Just(Kind::Compacted),
        Just(Kind::Finding),
        Just(Kind::Evidence),
        Just(Kind::Navigation)
    ]
    .boxed()
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let axis = prop_oneof![
        Just(Axis::Run),
        Just(Axis::Workspace),
        Just(Axis::Content),
        Just(Axis::Repository),
        Just(Axis::Execution),
        Just(Axis::Conversation),
        Just(Axis::Checkpoint)
    ];
    let operation = prop_oneof![
        5=>(any::<u8>(),prop::option::of(any::<u8>())).prop_map(|(index,value)|Operation::Source{index,value}),
        4=>(axis,any::<u8>()).prop_map(|(axis,value)|Operation::Axis{axis,value}),
        3=>any::<u8>().prop_map(|index|Operation::Clarify{index}),
        2=>Just(Operation::ProviderFailure),
        4=>(any::<u8>(),any::<u8>()).prop_map(|(index,material)|Operation::Capture{index,material}),
        1=>Just(Operation::RejectRole),1=>Just(Operation::RejectCurrent),2=>Just(Operation::Reconstruct),2=>Just(Operation::Inspect),
    ];
    (
        0_u8..3,
        prop::collection::vec(
            (kind_strategy(), 1_u8..16, any::<u16>(), any::<u8>()),
            3..=max_steps.clamp(3, 12),
        ),
        prop::collection::vec(operation, 0..=max_steps.min(256)),
    )
        .prop_map(|(role, raw, operations)| Case {
            role,
            sections: raw
                .into_iter()
                .enumerate()
                .map(|(index, (kind, sources, deps, material))| Section {
                    kind: match index {
                        0 => Kind::Inventory,
                        1 => Kind::FileMap,
                        2 => Kind::Requirements,
                        _ => kind,
                    },
                    sources: (0..4).filter(|source| sources & (1 << source) != 0).collect(),
                    dependencies: (0..index).filter(|prior| deps & (1 << prior) != 0).collect(),
                    material,
                })
                .collect(),
            operations,
        })
        .boxed()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Candidate {
    run: u8,
    workspace: u8,
    content: u8,
    repository: u8,
    execution: Option<u8>,
    conversation: u64,
    checkpoint: u64,
}
impl Candidate {
    fn production(&self) -> Result<CandidateIdentity, String> {
        let mut run = [1; 16];
        run[15] = self.run;
        let mut workspace = [2; 16];
        workspace[15] = self.workspace;
        CandidateIdentity::new(
            RunId::new(run).map_err(|error| format!("{error:?}"))?,
            WorkspaceId::new(workspace).map_err(|error| format!("{error:?}"))?,
            digest(self.content),
            digest(self.repository),
            self.execution.map(digest),
            self.conversation,
            self.checkpoint,
        )
        .map_err(|error| format!("{error:?}"))
    }
    fn same_candidate(&self, other: &Self) -> bool {
        self.run == other.run
            && self.workspace == other.workspace
            && self.content == other.content
            && self.repository == other.repository
            && self.execution == other.execution
            && self.conversation == other.conversation
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct BoundSection {
    section: Section,
    sources: Vec<(usize, u8)>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    candidate: Candidate,
    role: u8,
    sections: Vec<BoundSection>,
}
struct Current {
    candidate: Candidate,
    sources: [Option<u8>; 4],
    change: KnowledgeChange,
    targets: Vec<usize>,
}
fn digest(value: u8) -> Sha256Digest {
    peritus_codec::sha256(&[value; 8])
}
fn role(value: u8) -> HarnessRole {
    match value % 3 {
        0 => HarnessRole::Writer,
        1 => HarnessRole::Reviewer,
        _ => HarnessRole::Fixer,
    }
}
fn limits() -> Result<KnowledgeLimits, String> {
    KnowledgeLimits::new(64, 64, 64, 64).map_err(|error| error.to_string())
}
fn section_id(index: usize) -> Result<KnowledgeSectionId, String> {
    KnowledgeSectionId::new(((index as u128) + 1).to_be_bytes()).map_err(|error| error.to_string())
}
fn source_id(index: usize) -> Result<KnowledgeSourceId, String> {
    KnowledgeSourceId::new(((index as u128) + 1).to_be_bytes()).map_err(|error| error.to_string())
}
fn production_sources(sources: &[(usize, u8)]) -> Result<Vec<SourceDigest>, String> {
    sources
        .iter()
        .map(|(index, value)| Ok(SourceDigest::new(source_id(*index)?, digest(*value))))
        .collect()
}
fn build_snapshot(
    snapshot: &Snapshot,
    binding_role: Option<u8>,
) -> Result<RunKnowledgeSnapshot, String> {
    let candidate = snapshot.candidate.production()?;
    let sections = snapshot
        .sections
        .iter()
        .enumerate()
        .map(|(index, bound)| {
            let binding = KnowledgeBinding::new(
                candidate,
                role(binding_role.unwrap_or(snapshot.role)),
                snapshot.candidate.checkpoint,
                production_sources(&bound.sources)?,
                limits()?,
            )
            .map_err(|error| error.to_string())?;
            KnowledgeSection::new(
                section_id(index)?,
                bound.section.kind.production(),
                digest(bound.section.material),
                binding,
                bound
                    .section
                    .dependencies
                    .iter()
                    .map(|index| section_id(*index))
                    .collect::<Result<_, _>>()?,
                limits()?,
            )
            .map_err(|error| error.to_string())
        })
        .collect::<Result<_, _>>()?;
    RunKnowledgeSnapshot::new(
        candidate,
        role(snapshot.role),
        section_id(0)?,
        section_id(1)?,
        section_id(2)?,
        sections,
        limits()?,
    )
    .map_err(|error| error.to_string())
}
fn request(
    current: &Current,
    change: KnowledgeChange,
    targets: &[usize],
) -> Result<InvalidationRequest, String> {
    let sources = current
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.map(|value| (index, value)))
        .collect::<Vec<_>>();
    let state = CurrentKnowledgeState::new(
        current.candidate.production()?,
        production_sources(&sources)?,
        limits()?,
    )
    .map_err(|error| error.to_string())?;
    InvalidationRequest::new(
        state,
        change,
        targets.iter().map(|index| section_id(*index)).collect::<Result<_, _>>()?,
    )
    .map_err(|error| error.to_string())
}
fn fresh_snapshot(case: &Case, templates: &[Section], current: &Current) -> Snapshot {
    Snapshot {
        candidate: current.candidate.clone(),
        role: case.role,
        sections: templates
            .iter()
            .map(|section| {
                let mut sources: Vec<_> = section
                    .sources
                    .iter()
                    .filter_map(|index| current.sources[*index].map(|value| (*index, value)))
                    .collect();
                if sources.is_empty() {
                    sources.push((0, current.sources[0].unwrap_or(1)));
                }
                BoundSection { section: section.clone(), sources }
            })
            .collect(),
    }
}

/// Declarative independent seed conditions followed by reverse-graph reachability.
fn expected(
    snapshot: &Snapshot,
    current: &Current,
    change: KnowledgeChange,
    targets: &[usize],
) -> Vec<Option<InvalidationReason>> {
    let source_catalog: BTreeMap<_, _> = current
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.map(|value| (index, value)))
        .collect();
    let lineage = snapshot.candidate.run != current.candidate.run
        || snapshot.candidate.workspace != current.candidate.workspace;
    let same_candidate = snapshot.candidate.same_candidate(&current.candidate);
    let named = change == KnowledgeChange::UserClarification;
    let mut reasons: Vec<_> = snapshot
        .sections
        .iter()
        .enumerate()
        .map(|(index, bound)| {
            let conditions = [
                (lineage, InvalidationReason::CandidateLineageChanged),
                (
                    snapshot.candidate.checkpoint > current.candidate.checkpoint,
                    InvalidationReason::FutureObservation,
                ),
                (
                    bound.sources.iter().any(|(key, value)| source_catalog.get(key) != Some(value)),
                    InvalidationReason::SourceChanged,
                ),
                (named && targets.contains(&index), InvalidationReason::UserClarification),
                (
                    bound.section.kind.conversation()
                        && snapshot.candidate.conversation != current.candidate.conversation
                        && !named,
                    InvalidationReason::ConversationRevisionChanged,
                ),
                (
                    bound.section.kind.candidate() && !same_candidate,
                    InvalidationReason::CandidateRevisionChanged,
                ),
            ];
            conditions.into_iter().find_map(|(applies, reason)| applies.then_some(reason))
        })
        .collect();
    let mut reverse = vec![Vec::new(); snapshot.sections.len()];
    for (index, bound) in snapshot.sections.iter().enumerate() {
        for dependency in &bound.section.dependencies {
            reverse[*dependency].push(index);
        }
    }
    let mut queue: VecDeque<_> = reasons
        .iter()
        .enumerate()
        .filter(|(_, reason)| reason.is_some())
        .map(|(index, _)| index)
        .collect();
    while let Some(index) = queue.pop_front() {
        for dependent in &reverse[index] {
            if reasons[*dependent].is_none() {
                reasons[*dependent] = Some(InvalidationReason::DependencyInvalidated);
                queue.push_back(*dependent);
            }
        }
    }
    reasons
}
fn bump(metrics: &mut crate::Metrics, name: &str, value: u64) {
    *metrics.entry(name.into()).or_default() += value;
}
fn invalid_target(snapshot: &Snapshot, current: &Current) -> bool {
    current.change == KnowledgeChange::UserClarification
        && current.targets.iter().any(|index| {
            *index >= snapshot.sections.len()
                || !snapshot.sections[*index].section.kind.clarification()
        })
}
fn compare_plan(
    snapshot: &Snapshot,
    production: &RunKnowledgeSnapshot,
    current: &Current,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let request = request(current, current.change, &current.targets)?;
    let result = plan_invalidation(production, &request);
    if invalid_target(snapshot, current) {
        if !matches!(result,Err(error)if error.kind()==KnowledgeErrorKind::InvalidClarificationTarget)
        {
            return Err(format!("invalid clarification target accepted: {result:?}"));
        }
        bump(metrics, "invalid_clarification_rejections", 1);
        // A failed pure request must not poison the retained snapshot or a later valid request.
        let same = request_for_same(current)?;
        let after = plan_invalidation(production, &same).map_err(|error| error.to_string())?;
        verify_plan(snapshot, current, KnowledgeChange::SameRevision, &[], &after, metrics)?;
        bump(metrics, "continued_after_rejection", 1);
        return Ok(());
    }
    let plan = result.map_err(|error| error.to_string())?;
    verify_plan(snapshot, current, current.change, &current.targets, &plan, metrics)
}
fn request_for_same(current: &Current) -> Result<InvalidationRequest, String> {
    request(current, KnowledgeChange::SameRevision, &[])
}
fn verify_plan(
    snapshot: &Snapshot,
    current: &Current,
    change: KnowledgeChange,
    targets: &[usize],
    plan: &peritus_run_knowledge::InvalidationPlan,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let expected = expected(snapshot, current, change, targets);
    if plan.entries().len() != expected.len()
        || plan.role() != role(snapshot.role)
        || plan.candidate() != &current.candidate.production()?
    {
        return Err("knowledge plan changed role/candidate/count".into());
    }
    for (index, (entry, reason)) in plan.entries().iter().zip(&expected).enumerate() {
        let decision = reason.map_or(ReuseDecision::Reuse, ReuseDecision::Invalidate);
        if entry.section_id() != section_id(index)?
            || entry.decision() != decision
            || plan.is_reused(section_id(index)?) != reason.is_none()
        {
            return Err(format!("section {index}: {:?}, expected {decision:?}", entry.decision()));
        }
        bump(
            metrics,
            match reason {
                None => "reused_sections",
                Some(InvalidationReason::DependencyInvalidated) => {
                    "transitively_invalidated_sections"
                }
                Some(_) => "directly_invalidated_sections",
            },
            1,
        );
    }
    let invalidated = expected.iter().filter(|reason| reason.is_some()).count();
    let accounting = plan.accounting();
    if accounting.total() != expected.len()
        || accounting.invalidated() != invalidated
        || accounting.reused() != expected.len() - invalidated
    {
        return Err("invalidation accounting disagrees with source model".into());
    }
    bump(metrics, "invalidation_plans", 1);
    Ok(())
}
fn compare_delta(
    previous: &Snapshot,
    current_snapshot: &Snapshot,
    current: &Current,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let old = build_snapshot(previous, None)?;
    let new = build_snapshot(current_snapshot, None)?;
    let request = request_for_same(current)?;
    let plan = plan_delta_packet(&old, &new, &request).map_err(|error| error.to_string())?;
    let prior = expected(previous, current, KnowledgeChange::SameRevision, &[]);
    let mut counts = [0_usize; 3];
    if plan.entries().len() != current_snapshot.sections.len()
        || plan.role() != role(current_snapshot.role)
        || plan.candidate() != &current.candidate.production()?
    {
        return Err("delta role/candidate/count mismatch".into());
    }
    for (index, (actual, new_bound)) in
        plan.entries().iter().zip(&current_snapshot.sections).enumerate()
    {
        let old_bound = &previous.sections[index];
        let material_equal = old_bound.section.kind == new_bound.section.kind
            && old_bound.section.material == new_bound.section.material
            && old_bound.sources == new_bound.sources
            && old_bound.section.dependencies == new_bound.section.dependencies;
        let delivery = if new_bound.section.kind.navigation() {
            counts[2] += 1;
            DeltaDelivery::Navigation
        } else if prior[index].is_none() && material_equal {
            counts[1] += 1;
            DeltaDelivery::CurrentReference
        } else {
            counts[0] += 1;
            DeltaDelivery::ChangedFact
        };
        if actual.section_id() != section_id(index)? || actual.delivery() != delivery {
            return Err(format!(
                "delta section {index}: {:?}, expected {delivery:?}",
                actual.delivery()
            ));
        }
    }
    let actual = plan.accounting();
    if [actual.changed_facts(), actual.current_references(), actual.navigation_sections()] != counts
        || actual.invalidated_prior_sections()
            != prior.iter().filter(|reason| reason.is_some()).count()
    {
        return Err("delta accounting mismatch".into());
    }
    bump(metrics, "delta_packets", 1);
    bump(metrics, "changed_facts", counts[0] as u64);
    bump(metrics, "current_references", counts[1] as u64);
    bump(metrics, "navigation_entries", counts[2] as u64);
    Ok(())
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.role >= 3
        || case.sections.len() < 3
        || case.sections.len() > 32
        || case.operations.len() > 512
        || case.sections[0].kind != Kind::Inventory
        || case.sections[1].kind != Kind::FileMap
        || case.sections[2].kind != Kind::Requirements
    {
        return Err("invalid knowledge case base sections/bounds".into());
    }
    for (index, section) in case.sections.iter().enumerate() {
        if section.sources.is_empty()
            || section.sources.iter().any(|source| *source >= 4)
            || section.sources.windows(2).any(|pair| pair[0] >= pair[1])
            || section.dependencies.iter().any(|dependency| *dependency >= index)
            || section.dependencies.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err("noncanonical case source/dependency list".into());
        }
    }
    let mut metrics = crate::Metrics::new();
    let mut templates = case.sections.clone();
    let mut current = Current {
        candidate: Candidate {
            run: 1,
            workspace: 1,
            content: 1,
            repository: 1,
            execution: None,
            conversation: 1,
            checkpoint: 64,
        },
        sources: [Some(1), Some(2), Some(3), Some(4)],
        change: KnowledgeChange::SameRevision,
        targets: Vec::new(),
    };
    let mut saved = fresh_snapshot(case, &templates, &current);
    let mut production = build_snapshot(&saved, None)?;
    compare_plan(&saved, &production, &current, &mut metrics)?;
    compare_delta(&saved, &saved, &current, &mut metrics)?;
    for (step, operation) in case.operations.iter().enumerate() {
        let result = (|| {
            match *operation {
                Operation::Source { index, value } => {
                    let index = 1 + usize::from(index) % 3;
                    let changed = current.sources[index] != value;
                    current.sources[index] = value;
                    current.change = KnowledgeChange::SourceChanged;
                    current.targets.clear();
                    if changed {
                        bump(&mut metrics, "source_changes", 1);
                        if value.is_none() {
                            bump(&mut metrics, "missing_sources", 1);
                        } else {
                            bump(&mut metrics, "same_size_source_changes", 1);
                        }
                    }
                }
                Operation::Axis { axis, value } => {
                    match axis {
                        Axis::Run => current.candidate.run = value,
                        Axis::Workspace => current.candidate.workspace = value,
                        Axis::Content => current.candidate.content = value,
                        Axis::Repository => current.candidate.repository = value,
                        Axis::Execution => {
                            current.candidate.execution =
                                if value % 2 == 0 { None } else { Some(value) }
                        }
                        Axis::Conversation => current.candidate.conversation = u64::from(value),
                        Axis::Checkpoint => current.candidate.checkpoint = u64::from(value) + 1,
                    };
                    current.change = if matches!(axis, Axis::Conversation) {
                        KnowledgeChange::ConversationRevision
                    } else {
                        KnowledgeChange::CandidateRevision
                    };
                    current.targets.clear();
                    bump(
                        &mut metrics,
                        match axis {
                            Axis::Run => "run_axis_changes",
                            Axis::Workspace => "workspace_axis_changes",
                            Axis::Content => "content_axis_changes",
                            Axis::Repository => "repository_axis_changes",
                            Axis::Execution => "execution_axis_changes",
                            Axis::Conversation => "conversation_axis_changes",
                            Axis::Checkpoint => "checkpoint_axis_changes",
                        },
                        1,
                    );
                }
                Operation::Clarify { index } => {
                    current.candidate.conversation += 1;
                    current.change = KnowledgeChange::UserClarification;
                    current.targets = vec![usize::from(index) % templates.len()];
                    bump(&mut metrics, "clarification_requests", 1);
                }
                Operation::ProviderFailure => {
                    current.change = KnowledgeChange::ProviderFailure;
                    current.targets.clear();
                    bump(&mut metrics, "provider_failures", 1);
                }
                Operation::Capture { index, material } => {
                    let index = usize::from(index) % templates.len();
                    if templates[index].material != material {
                        bump(&mut metrics, "same_size_material_changes", 1);
                    }
                    templates[index].material = material;
                    current.change = KnowledgeChange::SameRevision;
                    current.targets.clear();
                    let fresh = fresh_snapshot(case, &templates, &current);
                    compare_delta(&saved, &fresh, &current, &mut metrics)?;
                    saved = fresh;
                    production = build_snapshot(&saved, None)?;
                    bump(&mut metrics, "snapshot_refreshes", 1);
                }
                Operation::RejectRole => {
                    let mut other = fresh_snapshot(case, &templates, &current);
                    other.role = (case.role + 1) % 3;
                    let other_production = build_snapshot(&other, None)?;
                    if !matches!(plan_delta_packet(&production,&other_production,&request_for_same(&current)?),Err(error)if error.kind()==KnowledgeErrorKind::RoleMismatch)
                    {
                        return Err("cross-role delta was accepted".into());
                    }
                    let candidate = other.candidate.production()?;
                    let binding = KnowledgeBinding::new(
                        candidate,
                        role(case.role),
                        candidate.checkpoint_sequence(),
                        production_sources(&other.sections[0].sources)?,
                        limits()?,
                    )
                    .map_err(|error| error.to_string())?;
                    let mut bad_sections = other_production.sections().to_vec();
                    bad_sections[0] = KnowledgeSection::new(
                        section_id(0)?,
                        KnowledgeSectionKind::RepositoryInventory,
                        digest(1),
                        binding,
                        Vec::new(),
                        limits()?,
                    )
                    .map_err(|error| error.to_string())?;
                    if !matches!(RunKnowledgeSnapshot::new(candidate,role(other.role),section_id(0)?,section_id(1)?,section_id(2)?,bad_sections,limits()?),Err(error)if error.kind()==KnowledgeErrorKind::RoleMismatch)
                    {
                        return Err("cross-role bound section was accepted".into());
                    }
                    bump(&mut metrics, "role_isolation_rejections", 2);
                }
                Operation::RejectCurrent => {
                    let mut stale = fresh_snapshot(case, &templates, &current);
                    stale.sections[0].sources[0].1 = stale.sections[0].sources[0].1.wrapping_add(1);
                    let stale_production = build_snapshot(&stale, None)?;
                    if !matches!(plan_delta_packet(&production,&stale_production,&request_for_same(&current)?),Err(error)if error.kind()==KnowledgeErrorKind::CurrentSnapshotStale)
                    {
                        return Err("delta accepted stale current section".into());
                    }
                    bump(&mut metrics, "stale_current_rejections", 1);
                }
                Operation::Reconstruct => {
                    let reconstructed = build_snapshot(&saved, None)?;
                    if reconstructed != production {
                        return Err("checked snapshot reconstruction changed content".into());
                    }
                    production = reconstructed;
                    bump(&mut metrics, "checked_snapshot_reconstructions", 1);
                }
                Operation::Inspect => bump(&mut metrics, "explicit_inspections", 1),
            }
            compare_plan(&saved, &production, &current, &mut metrics)
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
        sections: vec![
            Section { kind: Kind::Inventory, sources: vec![0], dependencies: vec![], material: 1 },
            Section { kind: Kind::FileMap, sources: vec![1], dependencies: vec![0], material: 2 },
            Section {
                kind: Kind::Requirements,
                sources: vec![2],
                dependencies: vec![],
                material: 3,
            },
            Section { kind: Kind::Design, sources: vec![3], dependencies: vec![1, 2], material: 4 },
            Section { kind: Kind::Evidence, sources: vec![0], dependencies: vec![3], material: 5 },
        ],
        operations: vec![
            Operation::Inspect,
            Operation::Source { index: 0, value: Some(9) },
            Operation::Capture { index: 3, material: 7 },
        ],
    }
}
pub fn examples() -> Vec<Case> {
    let mut broad = calibration_case();
    broad.sections.push(Section {
        kind: Kind::Navigation,
        sources: vec![0],
        dependencies: vec![0],
        material: 6,
    });
    broad.sections.push(Section {
        kind: Kind::Compacted,
        sources: vec![0],
        dependencies: vec![],
        material: 7,
    });
    broad.operations.extend([
        Operation::RejectRole,
        Operation::RejectCurrent,
        Operation::Reconstruct,
        Operation::Clarify { index: 2 },
        Operation::ProviderFailure,
        Operation::Capture { index: 3, material: 8 },
        Operation::Clarify { index: 0 },
        Operation::Axis { axis: Axis::Checkpoint, value: 0 },
        Operation::Axis { axis: Axis::Run, value: 9 },
        Operation::Axis { axis: Axis::Workspace, value: 8 },
        Operation::Axis { axis: Axis::Content, value: 7 },
        Operation::Axis { axis: Axis::Repository, value: 6 },
        Operation::Axis { axis: Axis::Execution, value: 5 },
        Operation::Axis { axis: Axis::Conversation, value: 4 },
        Operation::Source { index: 1, value: None },
        Operation::Capture { index: 4, material: 9 },
        Operation::RejectCurrent,
        Operation::RejectRole,
    ]);
    let mut reviewer = broad.clone();
    reviewer.role = 1;
    let mut fixer = broad.clone();
    fixer.role = 2;
    vec![calibration_case(), broad, reviewer, fixer]
}
