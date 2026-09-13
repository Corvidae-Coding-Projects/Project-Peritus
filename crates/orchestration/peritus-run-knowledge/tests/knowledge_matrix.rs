//! Run-knowledge reuse, invalidation, delta, authority, role, and bound matrix.

mod support;

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    DeltaDelivery, InvalidationReason, InvalidationRequest, KnowledgeAuthority, KnowledgeChange,
    KnowledgeErrorKind, KnowledgeSectionKind, ReuseDecision, plan_delta_packet, plan_invalidation,
};
use support::{FixtureRevision, candidate, limits, section_id, snapshot, sources, state};

fn decision(plan: &peritus_run_knowledge::InvalidationPlan, byte: u8) -> ReuseDecision {
    plan.entries()
        .iter()
        .find(|entry| entry.section_id() == section_id(byte))
        .expect("planned section")
        .decision()
}

#[test]
fn same_revision_reuses_every_authoritative_section_deterministically() {
    let identity = candidate(20, 1, 1);
    let prior = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let current = prior.clone();
    let request =
        InvalidationRequest::new(state(identity, 11), KnowledgeChange::SameRevision, Vec::new())
            .expect("same revision request");

    let plan = plan_invalidation(&prior, &request).expect("reuse plan");
    assert_eq!(plan.accounting().total(), 8);
    assert_eq!(plan.accounting().reused(), 8);
    assert_eq!(plan.accounting().invalidated(), 0);

    let first = plan_delta_packet(&prior, &current, &request).expect("first delta");
    let second = plan_delta_packet(&prior, &current, &request).expect("second delta");
    assert_eq!(first, second);
    assert_eq!(first.accounting().current_references(), 6);
    assert_eq!(first.accounting().navigation_sections(), 2);
    assert_eq!(first.accounting().changed_facts(), 0);
}

#[test]
fn one_changed_source_invalidates_its_observations_and_dependents() {
    let identity = candidate(20, 1, 1);
    let prior = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let current = snapshot(identity, HarnessRole::Writer, 13, FixtureRevision::SourceChanged);
    let request =
        InvalidationRequest::new(state(identity, 13), KnowledgeChange::SourceChanged, Vec::new())
            .expect("source change request");

    let plan = plan_invalidation(&prior, &request).expect("source invalidation");
    assert_eq!(plan.accounting().reused(), 2);
    assert_eq!(plan.accounting().invalidated(), 6);
    assert_eq!(decision(&plan, 1), ReuseDecision::Invalidate(InvalidationReason::SourceChanged));
    assert_eq!(decision(&plan, 2), ReuseDecision::Invalidate(InvalidationReason::SourceChanged));
    assert_eq!(decision(&plan, 4), ReuseDecision::Reuse);

    let delta = plan_delta_packet(&prior, &current, &request).expect("source delta");
    assert_eq!(delta.accounting().changed_facts(), 4);
    assert_eq!(delta.accounting().current_references(), 2);
    assert_eq!(delta.accounting().navigation_sections(), 2);
    assert_eq!(delta.accounting().invalidated_prior_sections(), 6);
}

#[test]
fn clarification_invalidates_named_requirements_design_and_candidate_dependents() {
    let before = candidate(20, 1, 1);
    let after = candidate(20, 2, 2);
    let prior = snapshot(before, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let request = InvalidationRequest::new(
        state(after, 11),
        KnowledgeChange::UserClarification,
        vec![section_id(3), section_id(4)],
    )
    .expect("clarification request");
    let plan = plan_invalidation(&prior, &request).expect("clarification plan");

    assert_eq!(decision(&plan, 1), ReuseDecision::Reuse);
    assert_eq!(decision(&plan, 2), ReuseDecision::Reuse);
    assert_eq!(
        decision(&plan, 3),
        ReuseDecision::Invalidate(InvalidationReason::UserClarification),
    );
    assert_eq!(
        decision(&plan, 4),
        ReuseDecision::Invalidate(InvalidationReason::UserClarification),
    );
    assert_eq!(plan.accounting().invalidated(), 6);
}

#[test]
fn conversation_delta_invalidates_conversation_and_candidate_dependent_knowledge() {
    let before = candidate(20, 1, 1);
    let after = candidate(20, 2, 2);
    let prior = snapshot(before, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let current = snapshot(after, HarnessRole::Writer, 11, FixtureRevision::ConversationChanged);
    let request = InvalidationRequest::new(
        state(after, 11),
        KnowledgeChange::ConversationRevision,
        Vec::new(),
    )
    .expect("conversation delta request");
    let plan = plan_invalidation(&prior, &request).expect("conversation delta plan");

    assert_eq!(plan.accounting().reused(), 2);
    assert_eq!(plan.accounting().invalidated(), 6);
    assert_eq!(decision(&plan, 1), ReuseDecision::Reuse);
    assert_eq!(decision(&plan, 2), ReuseDecision::Reuse);
    assert_eq!(
        decision(&plan, 3),
        ReuseDecision::Invalidate(InvalidationReason::ConversationRevisionChanged),
    );
    let delta = plan_delta_packet(&prior, &current, &request).expect("conversation delta");
    assert_eq!(delta.accounting().current_references(), 2);
    assert_eq!(delta.accounting().changed_facts(), 4);
    assert_eq!(delta.accounting().navigation_sections(), 2);
}

#[test]
fn candidate_revision_invalidates_only_candidate_dependent_knowledge() {
    let before = candidate(20, 1, 1);
    let after = candidate(21, 1, 2);
    let prior = snapshot(before, HarnessRole::Fixer, 11, FixtureRevision::Baseline);
    let current = snapshot(after, HarnessRole::Fixer, 11, FixtureRevision::CandidateChanged);
    let request =
        InvalidationRequest::new(state(after, 11), KnowledgeChange::CandidateRevision, Vec::new())
            .expect("candidate revision request");
    let plan = plan_invalidation(&prior, &request).expect("candidate plan");

    assert_eq!(plan.accounting().reused(), 4);
    assert_eq!(plan.accounting().invalidated(), 4);
    for byte in 1..=4 {
        assert_eq!(decision(&plan, byte), ReuseDecision::Reuse);
    }
    let delta = plan_delta_packet(&prior, &current, &request).expect("candidate delta");
    assert_eq!(delta.accounting().current_references(), 4);
    assert_eq!(delta.accounting().changed_facts(), 2);
    assert_eq!(delta.accounting().navigation_sections(), 2);
}

#[test]
fn provider_failure_invalidates_no_repository_or_candidate_fact() {
    let identity = candidate(20, 1, 1);
    let prior = snapshot(identity, HarnessRole::Reviewer, 11, FixtureRevision::Baseline);
    let request =
        InvalidationRequest::new(state(identity, 11), KnowledgeChange::ProviderFailure, Vec::new())
            .expect("provider request");
    let plan = plan_invalidation(&prior, &request).expect("provider retry plan");

    assert_eq!(plan.accounting().reused(), 8);
    assert_eq!(plan.accounting().invalidated(), 0);
}

#[test]
fn role_views_cannot_be_reused_across_writer_and_reviewer() {
    let identity = candidate(20, 1, 1);
    let writer = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let reviewer = snapshot(identity, HarnessRole::Reviewer, 11, FixtureRevision::Baseline);
    let request =
        InvalidationRequest::new(state(identity, 11), KnowledgeChange::SameRevision, Vec::new())
            .expect("same revision request");

    assert_eq!(
        plan_delta_packet(&writer, &reviewer, &request).expect_err("cross-role delta").kind(),
        KnowledgeErrorKind::RoleMismatch,
    );
}

#[test]
fn oversized_inventory_and_invalid_clarification_targets_fail_closed() {
    let identity = candidate(20, 1, 1);
    let valid = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let too_small = peritus_run_knowledge::KnowledgeLimits::new(7, 64, 8, 8).expect("small limits");
    assert_eq!(
        peritus_run_knowledge::RunKnowledgeSnapshot::new(
            identity,
            HarnessRole::Writer,
            section_id(1),
            section_id(2),
            section_id(3),
            valid.sections().to_vec(),
            too_small,
        )
        .expect_err("oversized snapshot")
        .kind(),
        KnowledgeErrorKind::LimitExceeded,
    );

    let request = InvalidationRequest::new(
        state(identity, 11),
        KnowledgeChange::UserClarification,
        vec![section_id(1)],
    )
    .expect("structurally valid request");
    assert_eq!(
        plan_invalidation(&valid, &request)
            .expect_err("inventory is not a clarification target")
            .kind(),
        KnowledgeErrorKind::InvalidClarificationTarget,
    );
}

#[test]
fn summaries_are_navigation_only_and_never_authoritative_evidence() {
    let identity = candidate(20, 1, 1);
    let snapshot = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    for byte in [5, 8] {
        let section = snapshot.section(section_id(byte)).expect("navigation section");
        assert_eq!(section.authority(), KnowledgeAuthority::NavigationOnly);
        assert!(!section.can_satisfy_authoritative_evidence());
    }
    for byte in [1, 2, 3, 4, 6, 7] {
        let section = snapshot.section(section_id(byte)).expect("authoritative section");
        assert_eq!(section.authority(), KnowledgeAuthority::Authoritative);
        assert!(section.can_satisfy_authoritative_evidence());
    }
    assert_eq!(
        snapshot.section(section_id(8)).expect("summary").kind(),
        KnowledgeSectionKind::NavigationSummary
    );
    assert_eq!(sources(11).len(), 2);
    assert_eq!(limits().max_sections(), 32);
}

#[test]
fn delta_entries_remain_in_canonical_render_order() {
    let identity = candidate(20, 1, 1);
    let snapshot = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let request =
        InvalidationRequest::new(state(identity, 11), KnowledgeChange::SameRevision, Vec::new())
            .expect("same revision request");
    let packet = plan_delta_packet(&snapshot, &snapshot, &request).expect("delta packet");

    for (expected_id, entry) in (1_u8..).zip(packet.entries()) {
        assert_eq!(entry.section_id(), section_id(expected_id));
        if matches!(entry.delivery(), DeltaDelivery::Navigation) {
            assert!([5, 8].contains(&expected_id));
        }
    }
}
