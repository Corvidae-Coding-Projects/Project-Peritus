//! Deterministic production D1 plan used by the gate commit crash probe.

use peritus_gates::{GateCommand, GateCommandKind, GatePlan, GateTransition, start};
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
    AcceptanceSpecId, CommandId, EnvironmentId, EventId, GateId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use crate::DaemonError;

use super::super::{digest, identifier, qualification_error};

pub(super) struct GateFixture {
    pub(super) plan: GatePlan,
    pub(super) command: GateCommand,
    pub(super) transition: GateTransition,
}

pub(super) fn build(store: peritus_journal::StoreId) -> Result<GateFixture, DaemonError> {
    let environment = nominal(EnvironmentId::new, b"peritus/h1/gate/environment/v1\0", store)?;
    let gate_id = nominal(GateId::new, b"peritus/h1/gate/id/v1\0", store)?;
    let definition = definition(gate_id)?;
    let binding = definition
        .acceptance_binding(environment)
        .map_err(|_| invalid("bind gate quality definition"))?;
    let execution = GateExecutionPlan::new(
        binding.action(),
        binding.environment(),
        binding.inputs(),
        binding.parser(),
        binding.success_rule(),
        definition.timeout_millis(),
        binding.resources(),
        GateFreshnessScope::ExactRevisionTuple,
    )
    .map_err(|_| invalid("construct gate execution plan"))?;
    let gate = GateDefinition::new(gate_id, execution, Vec::new(), Vec::new())
        .map_err(|_| invalid("construct gate definition"))?;
    let acceptance = nominal(AcceptanceSpecId::new, b"peritus/h1/gate/acceptance/v1\0", store)?;
    let revision = RevisionTuple::new(
        acceptance,
        nominal(HarnessId::new, b"peritus/h1/gate/harness/v1\0", store)?,
        nominal(WorkspaceId::new, b"peritus/h1/gate/workspace/v1\0", store)?,
        Generation::first(),
        RevisionNumber::first(),
        nominal(PolicyId::new, b"peritus/h1/gate/policy/v1\0", store)?,
        nominal(ProviderProfileId::new, b"peritus/h1/gate/provider/v1\0", store)?,
    );
    let contract = contract(acceptance, gate, store)?;
    let catalog = CheckCatalog::from_explicit(vec![definition])
        .map_err(|_| invalid("construct gate quality catalog"))?;
    let run_id = nominal(RunId::new, b"peritus/h1/gate/run/v1\0", store)?;
    let plan = GatePlan::new(run_id, &contract, revision, &catalog, environment)
        .map_err(|_| invalid("bind production gate plan"))?;
    let command = GateCommand::new(
        nominal(CommandId::new, b"peritus/h1/gate/command/v1\0", store)?,
        nominal(EventId::new, b"peritus/h1/gate/event/v1\0", store)?,
        run_id,
        0,
        None,
        Sha256Digest::new([0; 32]),
        revision,
        GateCommandKind::StartRun {
            snapshot_digest: digest(b"peritus/h1/gate/snapshot/v1\0", store),
        },
    )
    .map_err(|_| invalid("construct gate start command"))?;
    let transition = start(&plan, &command).map_err(|_| invalid("reduce gate start command"))?;
    Ok(GateFixture { plan, command, transition })
}

fn definition(gate_id: GateId) -> Result<CheckDefinition, DaemonError> {
    CheckDefinition::new(
        "h1-gate-commit",
        gate_id,
        CheckSource::Explicit("h1-qualification".to_owned()),
        CheckRequirement::Required,
        "peritus-h1-gate",
        vec!["verify".to_owned()],
        None,
        EnvironmentProfile::new("h1-qualification")
            .map_err(|_| invalid("construct gate environment profile"))?,
        10_000,
        4_096,
        OutputParser::None,
        ExpectedSuccess::ExitCode(0),
    )
    .map_err(|_| invalid("construct gate quality definition"))
}

fn contract(
    acceptance: AcceptanceSpecId,
    gate: GateDefinition,
    store: peritus_journal::StoreId,
) -> Result<AcceptanceContract, DaemonError> {
    let mut categories = [
        ReviewCategory::new(digest(b"peritus/h1/gate/review/one/v1\0", store)),
        ReviewCategory::new(digest(b"peritus/h1/gate/review/two/v1\0", store)),
    ];
    categories.sort_unstable();
    let review = ReviewPolicy::new(
        categories.to_vec(),
        2,
        ReviewerIndependence::new(true, true, true, false, false, true),
        FindingSeverity::High,
    )
    .map_err(|_| invalid("construct gate review policy"))?;
    let evidence = categories
        .into_iter()
        .enumerate()
        .map(|(index, category)| {
            let tag = u8::try_from(index + 1).map_err(|_| invalid("derive gate evidence tag"))?;
            Ok(EvidenceRequirement::new(
                EvidenceRequirementId::new(tagged_digest(tag, store)),
                ContentReference::new(tagged_digest(tag + 16, store)),
                EvidenceSource::Review(category),
                ExportClassification::Internal,
            ))
        })
        .collect::<Result<Vec<_>, DaemonError>>()?;
    AcceptanceContract::new(
        acceptance,
        digest(b"peritus/h1/gate/contract/v1\0", store),
        documents(store),
        vec![Requirement::new(
            RequirementId::new(digest(b"peritus/h1/gate/requirement/v1\0", store)),
            ContentReference::new(digest(b"peritus/h1/gate/requirement-text/v1\0", store)),
        )],
        vec![Exclusion::new(ContentReference::new(digest(
            b"peritus/h1/gate/exclusion/v1\0",
            store,
        )))],
        vec![Assumption::new(ContentReference::new(digest(
            b"peritus/h1/gate/assumption/v1\0",
            store,
        )))],
        GateGraph::new(vec![gate]).map_err(|_| invalid("construct gate graph"))?,
        review,
        evidence,
        CompletionPolicy::new(1, 2).map_err(|_| invalid("construct gate completion policy"))?,
        HumanApprovalPolicy::NotRequired,
        WaiverPolicy::Forbidden,
    )
    .map_err(|_| invalid("construct gate acceptance contract"))
}

fn documents(store: peritus_journal::StoreId) -> ContractDocuments {
    let mut value = 32_u8;
    let mut next = || {
        value = value.saturating_add(1);
        ContentReference::new(tagged_digest(value, store))
    };
    ContractDocuments::new(next(), next(), next(), next(), next(), next(), next(), next())
}

fn tagged_digest(tag: u8, store: peritus_journal::StoreId) -> Sha256Digest {
    let mut bytes = *digest(b"peritus/h1/gate/tag/v1\0", store).as_bytes();
    bytes[31] = tag;
    Sha256Digest::new(bytes)
}

fn nominal<T>(
    constructor: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
    domain: &[u8],
    store: peritus_journal::StoreId,
) -> Result<T, DaemonError> {
    constructor(identifier(domain, store))
        .map_err(|_| invalid("derive gate qualification identity"))
}

fn invalid(operation: &'static str) -> DaemonError {
    qualification_error(operation)
}
