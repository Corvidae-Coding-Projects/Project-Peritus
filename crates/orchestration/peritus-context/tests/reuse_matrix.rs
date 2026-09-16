//! Grounded run-knowledge selection and deterministic rendering matrix.

mod support;

use peritus_codec::sha256;
use peritus_context::{
    AuthorityClass, ContentKind, ContextErrorKind, KnowledgeContextLink, Provenance,
    RequirementMode, TrustClass, build_reusable_context_selections,
};
use peritus_policy::ActorRole;
use peritus_role::{ContextClass, HarnessRole};
use peritus_run_knowledge::{
    CandidateIdentity, CurrentKnowledgeState, InvalidationRequest, KnowledgeBinding,
    KnowledgeChange, KnowledgeLimits, KnowledgeSection, KnowledgeSectionId, KnowledgeSectionKind,
    KnowledgeSourceId, RunKnowledgeSnapshot, SourceDigest, plan_delta_packet,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use support::{graph, id as context_id, node, writer_roles};

fn limits() -> KnowledgeLimits {
    KnowledgeLimits::new(32, 64, 8, 8).expect("knowledge limits")
}

fn section_id(byte: u8) -> KnowledgeSectionId {
    KnowledgeSectionId::new([byte; 16]).expect("section id")
}

fn source_id(byte: u8) -> KnowledgeSourceId {
    KnowledgeSourceId::new([byte; 16]).expect("source id")
}

const fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::new([byte; 32])
}

fn candidate() -> CandidateIdentity {
    candidate_at(1)
}

fn candidate_at(checkpoint_sequence: u64) -> CandidateIdentity {
    CandidateIdentity::new(
        RunId::new([31; 16]).expect("run"),
        WorkspaceId::new([32; 16]).expect("workspace"),
        digest(33),
        1,
        checkpoint_sequence,
    )
    .expect("candidate")
}

fn source_catalog(source_a_digest: u8) -> Vec<SourceDigest> {
    vec![
        SourceDigest::new(source_id(1), digest(source_a_digest)),
        SourceDigest::new(source_id(2), digest(12)),
    ]
}

const fn uses_source_a(byte: u8) -> bool {
    matches!(byte, 1 | 2 | 5 | 6 | 7 | 8)
}

fn section_text(byte: u8, source_changed: bool) -> String {
    let version = if source_changed && uses_source_a(byte) { "v2" } else { "v1" };
    format!("grounded knowledge section {byte} {version}")
}

fn knowledge_snapshot(source_a_digest: u8, source_changed: bool) -> RunKnowledgeSnapshot {
    knowledge_snapshot_for(candidate(), HarnessRole::Writer, source_a_digest, source_changed)
}

fn knowledge_snapshot_for(
    identity: CandidateIdentity,
    role: HarnessRole,
    source_a_digest: u8,
    source_changed: bool,
) -> RunKnowledgeSnapshot {
    let source_a = SourceDigest::new(source_id(1), digest(source_a_digest));
    let source_b = SourceDigest::new(source_id(2), digest(12));
    let definitions = [
        (1, KnowledgeSectionKind::RepositoryInventory, vec![source_a], Vec::new()),
        (2, KnowledgeSectionKind::RelevantFileMap, vec![source_a], vec![section_id(1)]),
        (3, KnowledgeSectionKind::LiteralRequirementLedger, vec![source_b], Vec::new()),
        (4, KnowledgeSectionKind::DesignSection, vec![source_b], vec![section_id(3)]),
        (5, KnowledgeSectionKind::CompactedToolObservation, vec![source_a], vec![section_id(1)]),
        (6, KnowledgeSectionKind::ResolvedFinding, vec![source_a], vec![section_id(5)]),
        (7, KnowledgeSectionKind::CandidateEvidenceIndex, vec![source_a], vec![section_id(6)]),
        (
            8,
            KnowledgeSectionKind::NavigationSummary,
            vec![source_a, source_b],
            vec![section_id(4), section_id(7)],
        ),
    ];
    let sections = definitions
        .into_iter()
        .map(|(byte, kind, sources, dependencies)| {
            let text = section_text(byte, source_changed);
            let binding =
                KnowledgeBinding::new(identity, role, 1, sources, limits()).expect("binding");
            KnowledgeSection::new(
                section_id(byte),
                kind,
                sha256(text.as_bytes()),
                binding,
                dependencies,
                limits(),
            )
            .expect("section")
        })
        .collect();
    RunKnowledgeSnapshot::new(
        identity,
        role,
        section_id(1),
        section_id(2),
        section_id(3),
        sections,
        limits(),
    )
    .expect("snapshot")
}

fn context_graph(source_changed: bool) -> peritus_context::ContextGraph {
    context_graph_for(source_changed, ContextClass::RepositorySource, &writer_roles())
}

fn context_graph_for(
    source_changed: bool,
    class: ContextClass,
    visibility: &peritus_context::RoleVisibility,
) -> peritus_context::ContextGraph {
    graph(
        (1..=8)
            .map(|byte| {
                let text = section_text(byte, source_changed);
                node(
                    byte,
                    text.as_str(),
                    Provenance::Repository,
                    AuthorityClass::NonAuthoritative,
                    TrustClass::Constrained,
                    class,
                    ContentKind::RepositorySource,
                    8,
                    u64::from(byte),
                    RequirementMode::Optional,
                    0,
                    visibility.clone(),
                    Vec::new(),
                )
            })
            .collect(),
    )
}

fn links() -> Vec<KnowledgeContextLink> {
    (1..=8).map(|byte| KnowledgeContextLink::new(section_id(byte), context_id(byte))).collect()
}

#[test]
fn deterministic_selection_retains_full_provenance_and_packet_order() {
    let snapshot = knowledge_snapshot(11, false);
    let state = CurrentKnowledgeState::new(candidate(), source_catalog(11), limits())
        .expect("current state");
    let request = InvalidationRequest::new(state, KnowledgeChange::SameRevision, Vec::new())
        .expect("request");
    let packet = plan_delta_packet(&snapshot, &snapshot, &request).expect("delta packet");
    let graph = context_graph(false);

    let first = build_reusable_context_selections(&graph, &snapshot, &packet, links().as_slice())
        .expect("first selection");
    let second = build_reusable_context_selections(&graph, &snapshot, &packet, links().as_slice())
        .expect("second selection");
    assert_eq!(first, second);
    assert_eq!(first.len(), 8);
    for (expected_id, selection) in (1_u8..).zip(&first) {
        assert_eq!(selection.node_id(), context_id(expected_id));
        assert_eq!(selection.section().id(), section_id(expected_id));
        assert_eq!(selection.section().binding().candidate(), &candidate());
        assert_eq!(selection.section().binding().role(), HarnessRole::Writer);
    }
}

#[test]
fn changed_source_requires_new_digest_bound_context_before_selection() {
    let prior = knowledge_snapshot(11, false);
    let current = knowledge_snapshot(13, true);
    let state = CurrentKnowledgeState::new(candidate(), source_catalog(13), limits())
        .expect("changed state");
    let request = InvalidationRequest::new(state, KnowledgeChange::SourceChanged, Vec::new())
        .expect("request");
    let packet = plan_delta_packet(&prior, &current, &request).expect("delta packet");

    assert_eq!(
        build_reusable_context_selections(
            &context_graph(false),
            &current,
            &packet,
            links().as_slice(),
        )
        .expect_err("old context bytes cannot represent changed source knowledge")
        .kind(),
        ContextErrorKind::KnowledgeContextDigestMismatch,
    );
    let selections = build_reusable_context_selections(
        &context_graph(true),
        &current,
        &packet,
        links().as_slice(),
    )
    .expect("fresh context selection");
    assert_eq!(selections.len(), 8);
}

#[test]
fn packet_binding_uses_the_complete_candidate_identity() {
    let snapshot = knowledge_snapshot(11, false);
    let later_candidate = candidate_at(2);
    let later = knowledge_snapshot_for(later_candidate, HarnessRole::Writer, 11, false);
    let state = CurrentKnowledgeState::new(later_candidate, source_catalog(11), limits())
        .expect("later state");
    let request = InvalidationRequest::new(state, KnowledgeChange::SameRevision, Vec::new())
        .expect("request");
    let packet = plan_delta_packet(&later, &later, &request).expect("later packet");

    assert_eq!(
        build_reusable_context_selections(
            &context_graph(false),
            &snapshot,
            &packet,
            links().as_slice(),
        )
        .expect_err("checkpoint mismatch must fail before entry admission")
        .kind(),
        ContextErrorKind::KnowledgeRoleMismatch,
    );
}

#[test]
fn link_admission_rejects_duplicate_missing_and_absent_nodes() {
    let snapshot = knowledge_snapshot(11, false);
    let state = CurrentKnowledgeState::new(candidate(), source_catalog(11), limits())
        .expect("current state");
    let request = InvalidationRequest::new(state, KnowledgeChange::SameRevision, Vec::new())
        .expect("request");
    let packet = plan_delta_packet(&snapshot, &snapshot, &request).expect("packet");
    let graph = context_graph(false);

    let mut too_many = links();
    too_many.push(KnowledgeContextLink::new(section_id(9), context_id(9)));
    assert_eq!(
        build_reusable_context_selections(&graph, &snapshot, &packet, too_many.as_slice())
            .expect_err("link count is snapshot bounded")
            .kind(),
        ContextErrorKind::TooManyNodes,
    );

    let mut duplicate = links();
    duplicate[1] = duplicate[0];
    assert_eq!(
        build_reusable_context_selections(&graph, &snapshot, &packet, duplicate.as_slice())
            .expect_err("duplicate link")
            .kind(),
        ContextErrorKind::DuplicateValue,
    );

    let mut missing = links();
    missing.remove(0);
    assert_eq!(
        build_reusable_context_selections(&graph, &snapshot, &packet, missing.as_slice())
            .expect_err("missing link")
            .kind(),
        ContextErrorKind::KnowledgeContextLinkMissing,
    );

    let mut absent_node = links();
    absent_node[0] = KnowledgeContextLink::new(section_id(1), context_id(9));
    let error =
        build_reusable_context_selections(&graph, &snapshot, &packet, absent_node.as_slice())
            .expect_err("linked node must exist");
    assert_eq!(error.kind(), ContextErrorKind::PlanNodeMissing);
    assert_eq!(error.node_id(), Some(context_id(9)));
}

#[test]
fn role_and_context_class_visibility_are_both_required() {
    let identity = candidate();
    let snapshot = knowledge_snapshot_for(identity, HarnessRole::Reviewer, 11, false);
    let state =
        CurrentKnowledgeState::new(identity, source_catalog(11), limits()).expect("reviewer state");
    let request = InvalidationRequest::new(state, KnowledgeChange::SameRevision, Vec::new())
        .expect("request");
    let packet = plan_delta_packet(&snapshot, &snapshot, &request).expect("reviewer packet");

    let actor_hidden = context_graph_for(false, ContextClass::RepositorySource, &writer_roles());
    assert_eq!(
        build_reusable_context_selections(&actor_hidden, &snapshot, &packet, links().as_slice(),)
            .expect_err("actor visibility")
            .kind(),
        ContextErrorKind::KnowledgeRoleMismatch,
    );

    let reviewer_visible = peritus_context::RoleVisibility::new(
        vec![ActorRole::Reviewer],
        peritus_context::ContextLimits::new(32, 64, 8, 8).expect("context limits"),
    )
    .expect("reviewer visibility");
    let class_hidden = context_graph_for(false, ContextClass::HiddenReasoning, &reviewer_visible);
    assert_eq!(
        build_reusable_context_selections(&class_hidden, &snapshot, &packet, links().as_slice(),)
            .expect_err("class visibility")
            .kind(),
        ContextErrorKind::KnowledgeRoleMismatch,
    );
}
