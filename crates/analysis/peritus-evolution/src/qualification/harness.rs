//! Exact baseline, candidate, and protected policy fixture.

mod receipt;

use peritus_harness::{
    GoverningHarnessBinding,
    domain::{
        AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentContents,
        ComponentDeclaration, ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind,
        ComponentLocation, ComponentOwnership, ComponentRequirements, GraphEnvironment,
        HarnessLimits, HarnessRevision, LineageSeed, ManifestDigest, MediaType, Owner, Provenance,
        SchemaInterval, SchemaVersion, SourcePath, TargetPath, VerifiedComponentContent,
    },
};
use peritus_journal::StoreId;
use peritus_types::{
    AcceptanceSpecId, Generation, PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple,
    WorkspaceId,
};

use crate::{
    EvolutionLimits, Objective, ProductionHarnessBinding, PromotionPolicy, PromotionPolicyBinding,
    PromotionThresholds,
};

use super::identity::{digest, invalid, nominal};
use receipt::receipt;

pub(super) struct HarnessFixture {
    pub(super) baseline_revision: HarnessRevision,
    pub(super) candidate_revision: HarnessRevision,
    pub(super) baseline: ProductionHarnessBinding,
    pub(super) candidate: ProductionHarnessBinding,
    pub(super) policy: PromotionPolicyBinding,
}

impl HarnessFixture {
    pub(super) fn build(store: StoreId) -> Result<Self, crate::EvolutionError> {
        let policy = policy()?;
        let policy_bytes = policy_component_bytes(&policy);
        if peritus_codec::sha256(&policy_bytes) != policy.digest() {
            return Err(invalid("qualification policy bytes differ from the typed policy"));
        }
        let (baseline_graph, baseline_contents) =
            graph_and_contents(&policy_bytes, b"qualification baseline role\n")?;
        let baseline_revision = HarnessRevision::genesis(
            LineageSeed::new(digest(b"peritus/h1/promotion/lineage/v1\0", store)),
            ManifestDigest::new(digest(b"peritus/h1/promotion/baseline-manifest/v1\0", store)),
            baseline_graph,
            &baseline_contents,
        )
        .map_err(|_| invalid("construct qualification baseline harness"))?;
        let (candidate_graph, candidate_contents) =
            graph_and_contents(&policy_bytes, b"qualification candidate role\n")?;
        let candidate_revision = HarnessRevision::successor(
            &baseline_revision,
            ManifestDigest::new(digest(b"peritus/h1/promotion/candidate-manifest/v1\0", store)),
            candidate_graph,
            &candidate_contents,
        )
        .map_err(|_| invalid("construct qualification candidate harness"))?;
        let baseline = production_binding(&baseline_revision, 2, 20, store)?;
        let candidate = production_binding(&candidate_revision, 3, 40, store)?;
        let strategy = ComponentId::new("evolution.strategy")
            .map_err(|_| invalid("construct evolution strategy component identity"))?;
        let policy = PromotionPolicyBinding::capture(&baseline_revision, &strategy, policy)
            .map_err(|_| invalid("capture protected qualification promotion policy"))?;
        Ok(Self { baseline_revision, candidate_revision, baseline, candidate, policy })
    }
}

fn policy() -> Result<PromotionPolicy, crate::EvolutionError> {
    let thresholds = PromotionThresholds::new(
        -10_000, 0, 0, 900_000, 1_000_000, 2_000_000, 1_000_000, 16_000, 8_000, true, true,
    )?;
    PromotionPolicy::new(
        thresholds,
        vec![Objective::PairedCorrectness, Objective::Reliability, Objective::Cost],
        Vec::new(),
        false,
        1,
        EvolutionLimits::compiled(),
    )
}

fn production_binding(
    revision: &HarnessRevision,
    workspace_revision: u64,
    seed: u8,
    store: StoreId,
) -> Result<ProductionHarnessBinding, crate::EvolutionError> {
    let workspace = WorkspaceId::new(nominal(b"peritus/h1/promotion/workspace/v1\0", store))
        .map_err(|_| invalid("construct qualification workspace identity"))?;
    let tuple = RevisionTuple::new(
        AcceptanceSpecId::new(nominal(b"peritus/h1/promotion/acceptance/v1\0", store))
            .map_err(|_| invalid("construct qualification acceptance identity"))?,
        revision.harness_id(),
        workspace,
        Generation::first(),
        RevisionNumber::new(workspace_revision)
            .map_err(|_| invalid("construct qualification workspace revision"))?,
        PolicyId::new(nominal(b"peritus/h1/promotion/policy/v1\0", store))
            .map_err(|_| invalid("construct qualification policy identity"))?,
        ProviderProfileId::new(nominal(b"peritus/h1/promotion/provider/v1\0", store))
            .map_err(|_| invalid("construct qualification provider identity"))?,
    );
    let receipt = receipt(revision, workspace, workspace_revision, seed, store)?;
    let governing = GoverningHarnessBinding::new(tuple, revision, &receipt)
        .map_err(|_| invalid("bind qualification governing harness"))?;
    ProductionHarnessBinding::capture(&governing)
        .map_err(|_| invalid("capture qualification production harness"))
}

fn graph_and_contents(
    policy_bytes: &[u8],
    role_bytes: &[u8],
) -> Result<(CheckedHarnessGraph, ComponentContents), crate::EvolutionError> {
    let entries = vec![
        declaration("evolution.strategy", ComponentKind::EvolutionStrategy, policy_bytes)?,
        declaration("role.primary", ComponentKind::RolePrompt, role_bytes)?,
    ];
    let verified = entries
        .iter()
        .zip([policy_bytes, role_bytes])
        .map(|(declaration, content)| {
            VerifiedComponentContent::new(declaration, content.to_vec())
                .map_err(|_| invalid("verify qualification component contents"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let graph = CheckedHarnessGraph::check(
        entries,
        &GraphEnvironment::new(Vec::new(), Vec::new())
            .map_err(|_| invalid("construct qualification graph environment"))?,
        HarnessLimits::compiled(),
    )
    .map_err(|_| invalid("check qualification harness graph"))?;
    let contents = ComponentContents::new(&graph, verified)
        .map_err(|_| invalid("construct qualification harness contents"))?;
    Ok((graph, contents))
}

fn declaration(
    id: &str,
    kind: ComponentKind,
    content: &[u8],
) -> Result<ComponentDeclaration, crate::EvolutionError> {
    let schema =
        SchemaVersion::new(1).map_err(|_| invalid("construct qualification component schema"))?;
    ComponentDeclaration::new(
        ComponentIdentity::new(
            ComponentId::new(id).map_err(|_| invalid("construct qualification component id"))?,
            kind,
            schema,
        ),
        ComponentLocation::new(
            SourcePath::new(format!(".peritus-harness/components/{id}"))
                .map_err(|_| invalid("construct qualification source path"))?,
            TargetPath::new(format!("runtime/{id}"))
                .map_err(|_| invalid("construct qualification target path"))?,
            MediaType::new("application/octet-stream")
                .map_err(|_| invalid("construct qualification media type"))?,
        ),
        ComponentIntegrity::new(
            u64::try_from(content.len())
                .map_err(|_| invalid("qualification component size overflows"))?,
            peritus_codec::sha256(content),
            None,
        ),
        ComponentOwnership::new(
            Owner::new("f0-qualification")
                .map_err(|_| invalid("construct qualification component owner"))?,
            Provenance::new("deterministic promotion crash qualification")
                .map_err(|_| invalid("construct qualification provenance"))?,
        ),
        ComponentRequirements::new(
            Vec::new(),
            CompatibilityContract::new(
                SchemaInterval::new(schema, schema)
                    .map_err(|_| invalid("construct qualification schema interval"))?,
                Vec::new(),
                Vec::new(),
            )
            .map_err(|_| invalid("construct qualification compatibility contract"))?,
            AuthoritySet::empty(),
            kind.protection_class(),
        ),
        HarnessLimits::compiled(),
    )
    .map_err(|_| invalid("construct qualification component declaration"))
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
        &policy.objectives().iter().map(|value| *value as u8 + 1).collect::<Vec<_>>(),
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
    output.extend_from_slice(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    output.extend_from_slice(value);
}
