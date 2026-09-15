//! Role-scoped C6 memory retrieval, context selection, and C5 rendering integration.

mod context_fixture;
mod memory_fixture;

use peritus_agent::{MemorySelection, prepare_context, render_messages};
use peritus_context::{
    AuthorityClass, ContentKind, ContextPlanId, Provenance, RequirementMode, TokenBudget,
    TrustClass,
};
use peritus_memory::{
    BasisPoints, ClaimType, ClaimTypeSet, Confidence, FeedbackPolicy, RankingWeights,
    RequiredFeatures, RetrievalFeatures, RetrievalLimits, RetrievalPolicy, RetrievalQuery,
    ScopePolicy,
};
use peritus_model_protocol::{ContentBlock, Message, ProtocolLimits, Role};
use peritus_policy::ActorRole;
use peritus_role::{ContextClass, RoleProfile};
use peritus_types::Sha256Digest;

#[test]
fn selected_sources_keep_exact_order_content_and_authority_at_the_c5_boundary() {
    let graph = context_fixture::graph(vec![
        context_fixture::node(
            1,
            "external",
            Provenance::External,
            AuthorityClass::NonAuthoritative,
            TrustClass::Untrusted,
            ContextClass::ToolObservation,
            ContentKind::ToolObservation,
            1,
            1,
            RequirementMode::Required,
            0,
            context_fixture::writer_roles(),
            Vec::new(),
        ),
        context_fixture::node(
            2,
            "user",
            Provenance::User,
            AuthorityClass::UserInstruction,
            TrustClass::Trusted,
            ContextClass::ActiveUserRequest,
            ContentKind::ActiveUserInstruction,
            1,
            1,
            RequirementMode::Required,
            0,
            context_fixture::writer_roles(),
            Vec::new(),
        ),
        context_fixture::node(
            3,
            "application",
            Provenance::Application,
            AuthorityClass::ApplicationPolicy,
            TrustClass::Trusted,
            ContextClass::ImmutablePolicy,
            ContentKind::ApplicationPolicy,
            1,
            1,
            RequirementMode::Required,
            0,
            context_fixture::writer_roles(),
            Vec::new(),
        ),
        context_fixture::node(
            4,
            "system",
            Provenance::System,
            AuthorityClass::SystemPolicy,
            TrustClass::Trusted,
            ContextClass::ImmutablePolicy,
            ContentKind::SystemPolicy,
            1,
            1,
            RequirementMode::Required,
            0,
            context_fixture::writer_roles(),
            Vec::new(),
        ),
    ]);
    let prepared = prepare_context(
        &graph,
        ActorRole::Writer,
        ContextPlanId::new(Sha256Digest::new([90; 32])),
        TokenBudget::new(20, 2, 2).expect("budget"),
        10,
        1_000,
        None,
    )
    .expect("context");

    let messages =
        render_messages(prepared.render(), ProtocolLimits::PRODUCTION).expect("render messages");
    assert_eq!(
        messages.iter().map(Message::role).collect::<Vec<_>>(),
        vec![Role::System, Role::Developer, Role::User, Role::User]
    );
    assert_eq!(
        messages.iter().map(message_text).collect::<Vec<_>>(),
        vec![
            "<peritus-context source=\"04040404040404040404040404040404\" class=\"immutable-policy\" provenance=\"system\" authority=\"system-policy\" trust=\"trusted\" digest=\"bbc5e661e106c6dcd8dc6dd186454c2fcba3c710fb4d8e71a60c93eaf077f073\">\nsystem\n</peritus-context>",
            "<peritus-context source=\"03030303030303030303030303030303\" class=\"immutable-policy\" provenance=\"application\" authority=\"application-policy\" trust=\"trusted\" digest=\"1fe289205936c3fdb61158223892c7a8bee6ff4dfa085ea1c094ce0294e32114\">\napplication\n</peritus-context>",
            "<peritus-context source=\"02020202020202020202020202020202\" class=\"active-user-request\" provenance=\"user\" authority=\"user-instruction\" trust=\"trusted\" digest=\"04f8996da763b7a969b1028ee3007569eaf3a635486ddab211d512c85b9df8fb\">\nuser\n</peritus-context>",
            "<peritus-context source=\"01010101010101010101010101010101\" class=\"tool-observation\" provenance=\"external\" authority=\"non-authoritative\" trust=\"untrusted\" digest=\"3c4623849a49a53911c4a3e48d8cead8a1858960bccdea7a1b978d73ec2f06d7\">\nexternal\n</peritus-context>",
        ]
    );
}

fn message_text(message: &Message) -> &str {
    match message.content() {
        [ContentBlock::Text(text)] => text.expose_for_wire(),
        _ => panic!("context message must contain exactly one text block"),
    }
}

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
