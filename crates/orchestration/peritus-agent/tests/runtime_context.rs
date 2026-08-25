//! Role-scoped C6 memory retrieval, context selection, and C5 rendering integration.

mod context_fixture;
mod memory_fixture;

use peritus_agent::{MemorySelection, prepare_context, render_messages};
use peritus_context::{
    AuthorityClass, ContextPlanId, Provenance, RequirementMode, TokenBudget, TrustClass,
};
use peritus_memory::{
    BasisPoints, ClaimType, ClaimTypeSet, Confidence, FeedbackPolicy, RankingWeights,
    RequiredFeatures, RetrievalFeatures, RetrievalLimits, RetrievalPolicy, RetrievalQuery,
    ScopePolicy,
};
use peritus_model_protocol::{ProtocolLimits, Role};
use peritus_policy::ActorRole;
use peritus_role::RoleProfile;
use peritus_types::Sha256Digest;

#[test]
fn selected_memory_stays_untrusted_evidence_and_renders_as_a_separate_message() {
    let graph = context_fixture::graph(vec![context_fixture::evidence_node(
        1,
        "implement the requested change",
        6,
        RequirementMode::Required,
        Vec::new(),
    )]);
    let records = vec![memory_fixture::make_record(memory_fixture::RecordOptions::new(2))];
    let policy = policy();
    let query = RetrievalQuery::new(
        memory_fixture::project_scope(1),
        RoleProfile::for_actor_role(ActorRole::Writer),
        memory_fixture::observation(10),
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        64,
    )
    .expect("query");
    let selection = MemorySelection::new(&records, &[], &policy, &query, 2, 20, 1);
    let prepared = prepare_context(
        &graph,
        ActorRole::Writer,
        ContextPlanId::new(Sha256Digest::new([91; 32])),
        TokenBudget::new(128, 8, 8).expect("budget"),
        8,
        8_192,
        Some(selection),
    )
    .expect("context");

    assert_eq!(prepared.memory().expect("memory plan").selected().len(), 1);
    let memory_node = prepared
        .graph()
        .nodes()
        .iter()
        .find(|node| node.provenance() == Provenance::Memory)
        .expect("memory node");
    assert_eq!(memory_node.authority(), AuthorityClass::NonAuthoritative);
    assert_eq!(memory_node.trust(), TrustClass::Untrusted);

    let messages =
        render_messages(prepared.render(), ProtocolLimits::PRODUCTION).expect("render messages");
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|message| message.role() == Role::User));
}

fn policy() -> RetrievalPolicy {
    let claims = ClaimTypeSet::new(vec![
        ClaimType::Fact,
        ClaimType::Preference,
        ClaimType::Procedure,
        ClaimType::Outcome,
        ClaimType::Warning,
        ClaimType::Constraint,
        ClaimType::Hypothesis,
    ])
    .expect("claims");
    let weight = |value| BasisPoints::new(value).expect("weight");
    let weights = RankingWeights::new(
        weight(1_000),
        weight(3_000),
        weight(2_000),
        weight(1_500),
        weight(1_500),
        weight(1_000),
    )
    .expect("weights");
    RetrievalPolicy::new(
        RetrievalLimits::new(8, Confidence::new(0).expect("confidence"), None).expect("limits"),
        claims,
        weights,
        FeedbackPolicy::new(None, None),
        ScopePolicy::Exact,
    )
}
