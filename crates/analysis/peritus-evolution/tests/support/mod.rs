#![allow(dead_code, reason = "shared fixtures are compiled by independent integration tests")]

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_evolution::{
    CampaignCommand, CampaignCommandKind, CampaignState, EvolutionCampaignId, EvolutionLimits,
    Objective, PointerCommand, PointerCommandKind, ProductionHarnessBinding,
    ProductionHarnessState, PromotionPolicy, PromotionPolicyBinding, PromotionThresholds,
};
use peritus_harness::{
    GoverningHarnessBinding, MaterializationReceipt,
    domain::{
        AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentContents,
        ComponentDeclaration, ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind,
        ComponentLocation, ComponentOwnership, ComponentRequirements, GraphEnvironment,
        HarnessLimits, HarnessRevision, LineageSeed, ManifestDigest, MediaType, Owner, Provenance,
        SchemaInterval, SchemaVersion, SourcePath, TargetPath, VerifiedComponentContent,
    },
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProjectId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

mod store;

#[allow(unused_imports, reason = "each integration test consumes a subset of shared stores")]
pub use store::{Stores, open_journal};

pub const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

pub fn campaign_id() -> EvolutionCampaignId {
    EvolutionCampaignId::new(bytes(10)).expect("campaign identity")
}

pub fn project_id() -> ProjectId {
    ProjectId::new(bytes(11)).expect("project identity")
}

pub fn thresholds() -> PromotionThresholds {
    PromotionThresholds::new(
        -10_000, 0, 0, 900_000, 1_000_000, 2_000_000, 1_000_000, 16_000, 8_000, true, true,
    )
    .expect("promotion thresholds")
}

pub fn policy() -> PromotionPolicy {
    PromotionPolicy::new(
        thresholds(),
        vec![Objective::PairedCorrectness, Objective::Reliability, Objective::Cost],
        vec![ComponentKind::RolePrompt],
        false,
        4,
        EvolutionLimits::compiled(),
    )
    .expect("promotion policy")
}

pub struct HarnessFixture {
    pub baseline_revision: HarnessRevision,
    pub candidate_revision: HarnessRevision,
    pub baseline: ProductionHarnessBinding,
    pub candidate: ProductionHarnessBinding,
    pub policy: PromotionPolicyBinding,
}

impl HarnessFixture {
    pub fn new() -> Self {
        let policy = policy();
        let policy_bytes = policy_component_bytes(&policy);
        assert_eq!(
            peritus_codec::sha256(&policy_bytes),
            policy.digest(),
            "fixture must bind the exact typed policy bytes",
        );
        let baseline_role = b"baseline role\n".to_vec();
        let candidate_role = b"candidate role\n".to_vec();
        let (baseline_graph, baseline_contents) = graph_and_contents(&policy_bytes, &baseline_role);
        let baseline_revision = HarnessRevision::genesis(
            LineageSeed::new(digest(12)),
            ManifestDigest::new(digest(13)),
            baseline_graph,
            &baseline_contents,
        )
        .expect("baseline harness revision");
        let (candidate_graph, candidate_contents) =
            graph_and_contents(&policy_bytes, &candidate_role);
        let candidate_revision = HarnessRevision::successor(
            &baseline_revision,
            ManifestDigest::new(digest(14)),
            candidate_graph,
            &candidate_contents,
        )
        .expect("candidate harness revision");
        let baseline = production_binding(&baseline_revision, 2, 20);
        let candidate = production_binding(&candidate_revision, 3, 40);
        let strategy = ComponentId::new("evolution.strategy").expect("strategy component");
        let policy = PromotionPolicyBinding::capture(&baseline_revision, &strategy, policy)
            .expect("protected policy binding");
        Self { baseline_revision, candidate_revision, baseline, candidate, policy }
    }
}

pub fn campaign_genesis(fixture: &HarnessFixture) -> CampaignCommand {
    CampaignCommand::new(
        CommandId::new(bytes(60)).expect("campaign command"),
        EventId::new(bytes(61)).expect("campaign event"),
        campaign_id(),
        0,
        None,
        digest(0),
        fixture.policy.policy().digest(),
        CampaignCommandKind::CreateCampaign {
            project_id: project_id(),
            baseline: fixture.baseline,
            policy: fixture.policy.clone(),
            limits: EvolutionLimits::compiled(),
        },
    )
    .expect("campaign genesis")
}

pub fn next_campaign(
    state: &CampaignState,
    command_seed: u8,
    event_seed: u8,
    kind: CampaignCommandKind,
) -> CampaignCommand {
    CampaignCommand::new(
        CommandId::new(bytes(command_seed)).expect("campaign command"),
        EventId::new(bytes(event_seed)).expect("campaign event"),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event()),
        state.state_digest(),
        state.policy().policy().digest(),
        kind,
    )
    .expect("next campaign command")
}

pub fn pointer_genesis(
    fixture: &HarnessFixture,
    evidence_artifact: Sha256Digest,
    evidence_digest: Sha256Digest,
) -> PointerCommand {
    PointerCommand::new(
        CommandId::new(bytes(70)).expect("pointer command"),
        EventId::new(bytes(71)).expect("pointer event"),
        project_id(),
        0,
        None,
        0,
        digest(0),
        fixture.policy.digest(),
        PointerCommandKind::InitializeProductionHarness {
            initial: fixture.baseline,
            policy: fixture.policy.clone(),
            limits: EvolutionLimits::compiled(),
            evidence_artifact,
            evidence_digest,
        },
    )
    .expect("pointer genesis")
}

pub fn next_pointer(
    state: &ProductionHarnessState,
    command_seed: u8,
    event_seed: u8,
    kind: PointerCommandKind,
) -> PointerCommand {
    PointerCommand::new(
        CommandId::new(bytes(command_seed)).expect("pointer command"),
        EventId::new(bytes(event_seed)).expect("pointer event"),
        state.project_id(),
        state.sequence(),
        Some(state.last_event()),
        state.generation(),
        state.state_digest(),
        state.policy().digest(),
        kind,
    )
    .expect("next pointer command")
}

pub fn revision_tuple() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(90)).expect("acceptance specification"),
        HarnessId::new(bytes(91)).expect("harness"),
        WorkspaceId::new(bytes(92)).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(93)).expect("policy"),
        ProviderProfileId::new(bytes(94)).expect("provider profile"),
    )
}

fn production_binding(
    revision: &HarnessRevision,
    workspace_revision: u64,
    seed: u8,
) -> ProductionHarnessBinding {
    let workspace_id = WorkspaceId::new(bytes(15)).expect("workspace identity");
    let tuple = RevisionTuple::new(
        AcceptanceSpecId::new(bytes(16)).expect("acceptance specification"),
        revision.harness_id(),
        workspace_id,
        Generation::first(),
        RevisionNumber::new(workspace_revision).expect("workspace revision"),
        PolicyId::new(bytes(17)).expect("policy identity"),
        ProviderProfileId::new(bytes(18)).expect("provider profile identity"),
    );
    let receipt = receipt(revision, workspace_id, workspace_revision, seed);
    let governing = GoverningHarnessBinding::new(tuple, revision, &receipt)
        .expect("exact governing harness binding");
    ProductionHarnessBinding::capture(&governing).expect("production harness binding")
}

fn receipt(
    revision: &HarnessRevision,
    workspace_id: WorkspaceId,
    installed_revision: u64,
    seed: u8,
) -> MaterializationReceipt {
    let mut fields = CanonicalWriter::new(CodecLimits::PRODUCTION);
    fields.write_fixed(&bytes(seed)).expect("plan identity");
    fields.write_fixed(digest(seed.wrapping_add(1)).as_bytes()).expect("plan digest");
    fields.write_fixed(revision.harness_id().as_bytes()).expect("harness identity");
    fields.write_fixed(revision.digest().as_bytes()).expect("revision digest");
    fields.write_option_tag(false).expect("prior receipt tag");
    fields.write_fixed(digest(seed.wrapping_add(2)).as_bytes()).expect("patch identity");
    fields.write_fixed(&bytes(seed.wrapping_add(3))).expect("patch action");
    fields.write_fixed(digest(seed.wrapping_add(4)).as_bytes()).expect("patch authorization");
    fields.write_fixed(&bytes(seed.wrapping_add(5))).expect("candidate action");
    fields.write_fixed(digest(seed.wrapping_add(6)).as_bytes()).expect("candidate authorization");
    write_snapshot(
        &mut fields,
        workspace_id,
        installed_revision.checked_sub(1).expect("installed revision predecessor"),
        seed.wrapping_add(7),
    );
    write_snapshot(&mut fields, workspace_id, installed_revision, seed.wrapping_add(9));
    fields.write_fixed(&bytes(seed.wrapping_add(11))).expect("snapshot identity");
    fields.write_fixed(digest(seed.wrapping_add(12)).as_bytes()).expect("workspace manifest");
    fields.write_collection_len(0).expect("empty installed inventory");
    fields.write_u64(100).expect("start time");
    fields.write_u64(101).expect("completion time");
    fields.write_fixed(&bytes(seed.wrapping_add(13))).expect("causal event");

    let domain = b"peritus.harness.materialization-receipt.v1\0";
    let mut without_identity = CanonicalWriter::new(CodecLimits::PRODUCTION);
    without_identity.write_fixed(domain).expect("receipt domain");
    without_identity.write_fixed(fields.as_slice()).expect("receipt fields");
    let receipt_digest = peritus_codec::sha256(without_identity.as_slice());
    let mut receipt_id = [0_u8; 16];
    receipt_id.copy_from_slice(&receipt_digest.as_bytes()[..16]);
    receipt_id[0] |= 0x40;

    let mut encoded = CanonicalWriter::new(CodecLimits::PRODUCTION);
    encoded.write_fixed(domain).expect("receipt domain");
    encoded.write_fixed(&receipt_id).expect("receipt identity");
    encoded.write_fixed(receipt_digest.as_bytes()).expect("receipt digest");
    encoded.write_fixed(fields.as_slice()).expect("receipt fields");
    MaterializationReceipt::decode_canonical(&encoded.into_bytes())
        .expect("canonical materialization receipt")
}

fn write_snapshot(
    writer: &mut CanonicalWriter,
    workspace_id: WorkspaceId,
    revision: u64,
    seed: u8,
) {
    writer.write_fixed(workspace_id.as_bytes()).expect("snapshot workspace");
    writer.write_u64(Generation::first().get()).expect("snapshot generation");
    writer.write_u64(revision).expect("snapshot revision");
    writer.write_u8(2).expect("commit format");
    writer.write_bytes(&[seed; 32]).expect("commit object");
    writer.write_u8(2).expect("tree format");
    writer.write_bytes(&[seed.wrapping_add(1); 32]).expect("tree object");
}

fn graph_and_contents(
    policy_bytes: &[u8],
    role_bytes: &[u8],
) -> (CheckedHarnessGraph, ComponentContents) {
    let entries = vec![
        declaration("evolution.strategy", ComponentKind::EvolutionStrategy, policy_bytes),
        declaration("role.primary", ComponentKind::RolePrompt, role_bytes),
    ];
    let verified = entries
        .iter()
        .zip([policy_bytes, role_bytes])
        .map(|(declaration, content)| {
            VerifiedComponentContent::new(declaration, content.to_vec())
                .expect("verified component content")
        })
        .collect();
    let graph = CheckedHarnessGraph::check(
        entries,
        &GraphEnvironment::new(Vec::new(), Vec::new()).expect("graph environment"),
        HarnessLimits::compiled(),
    )
    .expect("checked harness graph");
    let contents = ComponentContents::new(&graph, verified).expect("complete harness contents");
    (graph, contents)
}

fn declaration(id: &str, kind: ComponentKind, content: &[u8]) -> ComponentDeclaration {
    let schema = SchemaVersion::new(1).expect("component schema");
    ComponentDeclaration::new(
        ComponentIdentity::new(ComponentId::new(id).expect("component identity"), kind, schema),
        ComponentLocation::new(
            SourcePath::new(format!(".peritus-harness/components/{id}")).expect("component source"),
            TargetPath::new(format!("runtime/{id}")).expect("component target"),
            MediaType::new("application/octet-stream").expect("component media type"),
        ),
        ComponentIntegrity::new(
            u64::try_from(content.len()).expect("component byte length"),
            peritus_codec::sha256(content),
            None,
        ),
        ComponentOwnership::new(
            Owner::new("f0-test").expect("component owner"),
            Provenance::new("deterministic integration fixture").expect("component provenance"),
        ),
        ComponentRequirements::new(
            Vec::new(),
            CompatibilityContract::new(
                SchemaInterval::new(schema, schema).expect("schema interval"),
                Vec::new(),
                Vec::new(),
            )
            .expect("compatibility contract"),
            AuthoritySet::empty(),
            kind.protection_class(),
        ),
        HarnessLimits::compiled(),
    )
    .expect("component declaration")
}

fn policy_component_bytes(policy: &PromotionPolicy) -> Vec<u8> {
    let thresholds = policy.thresholds();
    let mut semantic = Vec::new();
    semantic.extend_from_slice(&thresholds.minimum_paired_lower_millionths().to_be_bytes());
    semantic.extend_from_slice(&thresholds.maximum_critical_regressions().to_be_bytes());
    semantic.extend_from_slice(&thresholds.maximum_safety_failures().to_be_bytes());
    semantic.extend_from_slice(&thresholds.minimum_reliability_lower_millionths().to_be_bytes());
    semantic.extend_from_slice(&thresholds.minimum_attribution_coverage_millionths().to_be_bytes());
    semantic.extend_from_slice(&thresholds.maximum_latency_p95_micros().to_be_bytes());
    semantic.extend_from_slice(&thresholds.maximum_cost_mean_microunits().to_be_bytes());
    semantic.extend_from_slice(&thresholds.maximum_input_tokens_mean().to_be_bytes());
    semantic.extend_from_slice(&thresholds.maximum_output_tokens_mean().to_be_bytes());
    semantic.push(u8::from(thresholds.require_complete_trace()));
    semantic.push(u8::from(thresholds.require_complete_teardown()));
    push_len_prefixed(
        &mut semantic,
        &policy.objectives().iter().map(|value| objective_tag(*value)).collect::<Vec<_>>(),
    );
    push_len_prefixed(
        &mut semantic,
        &policy.review_required_kinds().iter().map(|value| value.tag()).collect::<Vec<_>>(),
    );
    semantic.push(u8::from(policy.allow_cross_lineage()));
    semantic.extend_from_slice(&policy.maximum_variants().to_be_bytes());

    let mut preimage = b"peritus.f0.promotion-policy.v1\0".to_vec();
    push_len_prefixed(&mut preimage, &semantic);
    preimage
}

fn push_len_prefixed(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&u64::try_from(value.len()).expect("fixture length").to_be_bytes());
    output.extend_from_slice(value);
}

const fn objective_tag(value: Objective) -> u8 {
    match value {
        Objective::PairedCorrectness => 1,
        Objective::CriticalRegressions => 2,
        Objective::SafetyFailures => 3,
        Objective::Reliability => 4,
        Objective::Latency => 5,
        Objective::Cost => 6,
        Objective::InputTokens => 7,
        Objective::OutputTokens => 8,
        Objective::AttributionCoverage => 9,
    }
}
