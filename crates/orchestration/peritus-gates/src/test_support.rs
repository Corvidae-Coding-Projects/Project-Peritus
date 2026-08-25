#![allow(clippy::unwrap_used, reason = "test fixtures use fixed valid values")]

use peritus_quality_policy::GateAttemptOrdinal;
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_tools_quality::{
    CheckCatalog, CheckDefinition, CheckRequirement, CheckSource, EnvironmentProfile,
    ExpectedSuccess, OutputParser,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, CommandId, EnvironmentId, EventId, GateExecutionId, GateId,
    Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId, RevisionNumber, RevisionTuple,
    RunId, Sha256Digest, WorkspaceId,
};

use crate::{
    ActiveAttempt, GateAttemptResult, GateCommand, GateCommandKind, GateEvidenceReceipt,
    GateOutcomeKind, GatePlan, GateRunState, RecoveryRequirement, RetryPermission,
};

pub struct Fixture {
    pub plan: GatePlan,
    pub run_id: RunId,
    pub revision: RevisionTuple,
    pub snapshot: Sha256Digest,
    pub first: GateId,
    pub second: GateId,
}

pub fn fixture(maximum_attempts: u16) -> Fixture {
    fixture_with_run(maximum_attempts, 15)
}

pub fn fixture_with_run(maximum_attempts: u16, run_seed: u8) -> Fixture {
    let environment = id(EnvironmentId::new, 7);
    let first = id(GateId::new, 1);
    let second = id(GateId::new, 2);
    let definitions = vec![definition(first, "gate-1"), definition(second, "gate-2")];
    let graph = GateGraph::new(vec![
        gate(&definitions[0], environment, Vec::new()),
        gate(&definitions[1], environment, vec![first]),
    ])
    .expect("valid gate DAG");
    let acceptance = id(AcceptanceSpecId::new, 10);
    let revision = RevisionTuple::new(
        acceptance,
        id(HarnessId::new, 11),
        id(WorkspaceId::new, 12),
        Generation::first(),
        RevisionNumber::first(),
        id(PolicyId::new, 13),
        id(ProviderProfileId::new, 14),
    );
    let contract = AcceptanceContract::new(
        acceptance,
        digest(90),
        documents(),
        vec![Requirement::new(RequirementId::new(digest(1)), content(21))],
        vec![Exclusion::new(content(31))],
        vec![Assumption::new(content(41))],
        graph,
        review_policy(),
        review_evidence(),
        CompletionPolicy::new(maximum_attempts, 2).expect("completion policy"),
        HumanApprovalPolicy::NotRequired,
        WaiverPolicy::Forbidden,
    )
    .expect("acceptance contract");
    let catalog = CheckCatalog::from_explicit(definitions).expect("quality catalog");
    let run_id = id(RunId::new, run_seed);
    let plan =
        GatePlan::new(run_id, &contract, revision, &catalog, environment).expect("bound gate plan");
    Fixture { plan, run_id, revision, snapshot: digest(91), first, second }
}

pub fn definition(gate_id: GateId, name: &str) -> CheckDefinition {
    CheckDefinition::new(
        name,
        gate_id,
        CheckSource::Explicit("acceptance-contract".to_owned()),
        CheckRequirement::Required,
        "quality-check",
        vec![name.to_owned()],
        None,
        EnvironmentProfile::new("quality-default").expect("profile"),
        10_000,
        1_024,
        OutputParser::None,
        ExpectedSuccess::ExitCode(0),
    )
    .expect("definition")
}

pub fn start_command(fixture: &Fixture, seed: u8) -> GateCommand {
    GateCommand::new(
        id(CommandId::new, seed),
        id(EventId::new, seed),
        fixture.run_id,
        0,
        None,
        digest(0),
        fixture.revision,
        GateCommandKind::StartRun { snapshot_digest: fixture.snapshot },
    )
    .expect("genesis command")
}

pub fn command(state: &GateRunState, seed: u8, kind: GateCommandKind) -> GateCommand {
    GateCommand::new(
        id(CommandId::new, seed),
        id(EventId::new, seed),
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.revision(),
        kind,
    )
    .expect("fenced command")
}

pub fn attempt(fixture: &Fixture, seed: u8, ordinal: u16) -> ActiveAttempt {
    attempt_with_action(fixture, seed, ordinal, id(ActionId::new, seed))
}

pub fn attempt_with_action(
    fixture: &Fixture,
    execution_seed: u8,
    ordinal: u16,
    action_id: ActionId,
) -> ActiveAttempt {
    ActiveAttempt::new(
        id(GateExecutionId::new, execution_seed),
        GateAttemptOrdinal::new(ordinal).expect("attempt ordinal"),
        action_id,
        digest(execution_seed.wrapping_add(1)),
        digest(execution_seed.wrapping_add(2)),
        fixture.snapshot,
    )
}

pub fn candidate_failure(gate_id: GateId, seed: u8) -> GateAttemptResult {
    GateAttemptResult::from_parts(
        gate_id,
        GateOutcomeKind::CandidateFailure,
        digest(seed),
        Some(digest(seed.wrapping_add(1))),
        Some(digest(seed.wrapping_add(2))),
        Some(id(ProcessId::new, seed)),
        Vec::new(),
        RetryPermission::Never,
        RecoveryRequirement::None,
    )
    .expect("candidate failure")
}

pub fn passing(gate_id: GateId, seed: u8) -> GateAttemptResult {
    GateAttemptResult::from_parts(
        gate_id,
        GateOutcomeKind::Passed,
        digest(seed),
        Some(digest(seed.wrapping_add(1))),
        Some(digest(seed.wrapping_add(2))),
        Some(id(ProcessId::new, seed)),
        Vec::new(),
        RetryPermission::Never,
        RecoveryRequirement::None,
    )
    .expect("passing result")
}

pub fn retryable(gate_id: GateId, seed: u8) -> GateAttemptResult {
    GateAttemptResult::from_parts(
        gate_id,
        GateOutcomeKind::InfrastructureFailure,
        digest(seed),
        None,
        Some(digest(seed.wrapping_add(2))),
        Some(id(ProcessId::new, seed)),
        Vec::new(),
        RetryPermission::AfterRecovery,
        RecoveryRequirement::ReconcileProcess,
    )
    .expect("retryable result")
}

pub fn empty_receipt(
    fixture: &Fixture,
    state: &GateRunState,
    gate_id: GateId,
    seed: u8,
) -> GateEvidenceReceipt {
    let planned = fixture.plan.gate(gate_id).expect("planned gate");
    let slot = state.slot(gate_id).expect("gate slot");
    let attempt = slot.active_attempt().expect("active attempt");
    let result = slot.last_result().expect("attempt result");
    let publication = crate::EvidencePublication::new(
        state.run_id(),
        gate_id,
        attempt,
        fixture.revision,
        slot.result_event().expect("result event"),
        u64::from(seed).saturating_add(1),
        result.tool_result_digest(),
        planned.required_evidence().to_vec(),
        result.artifacts().to_vec(),
    )
    .expect("exact publication");
    publication.receipt_from_records(Vec::new()).expect("empty exact receipt")
}

pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

fn gate(
    definition: &CheckDefinition,
    environment: EnvironmentId,
    dependencies: Vec<GateId>,
) -> GateDefinition {
    let binding = definition.acceptance_binding(environment).expect("acceptance binding");
    let plan = GateExecutionPlan::new(
        binding.action(),
        binding.environment(),
        binding.inputs(),
        binding.parser(),
        binding.success_rule(),
        definition.timeout_millis(),
        binding.resources(),
        GateFreshnessScope::ExactRevisionTuple,
    )
    .expect("gate execution plan");
    GateDefinition::new(definition.gate_id(), plan, dependencies, Vec::new())
        .expect("gate definition")
}

fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}

fn documents() -> ContractDocuments {
    ContractDocuments::new(
        content(70),
        content(71),
        content(72),
        content(73),
        content(74),
        content(75),
        content(76),
        content(77),
    )
}

fn review_policy() -> ReviewPolicy {
    ReviewPolicy::new(
        vec![ReviewCategory::new(digest(1)), ReviewCategory::new(digest(2))],
        2,
        ReviewerIndependence::new(true, true, true, false, false, true),
        FindingSeverity::High,
    )
    .expect("review policy")
}

fn review_evidence() -> Vec<EvidenceRequirement> {
    [1_u8, 2]
        .into_iter()
        .map(|seed| {
            EvidenceRequirement::new(
                EvidenceRequirementId::new(digest(seed)),
                content(seed.wrapping_add(50)),
                EvidenceSource::Review(ReviewCategory::new(digest(seed))),
                ExportClassification::Internal,
            )
        })
        .collect()
}

fn id<T>(
    constructor: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
    value: u8,
) -> T {
    constructor([value; 16]).expect("nonzero nominal identity")
}
