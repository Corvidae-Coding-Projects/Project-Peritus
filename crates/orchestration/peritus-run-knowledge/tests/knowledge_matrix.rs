//! Run-knowledge reuse, invalidation, delta, authority, role, and bound matrix.

mod support;

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CurrentKnowledgeState, DeltaDelivery, InvalidationReason, InvalidationRequest,
    KnowledgeAuthority, KnowledgeChange, KnowledgeErrorKind, KnowledgeSectionId,
    KnowledgeSectionKind, KnowledgeSourceId, ReuseDecision, SourceDigest, plan_delta_packet,
    plan_invalidation,
};
use support::{
    FixtureRevision, candidate, digest, limits, section_id, snapshot, source_id, sources, state,
};

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
fn planner_emits_one_ordered_entry_per_section_and_partitions_accounting() {
    let identity = candidate(20, 1, 1);
    let prior = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let request =
        InvalidationRequest::new(state(identity, 13), KnowledgeChange::SourceChanged, Vec::new())
            .expect("source change request");

    let plan = plan_invalidation(&prior, &request).expect("ordered invalidation plan");
    assert_eq!(plan.entries().len(), prior.sections().len());
    for (expected, entry) in prior.sections().iter().zip(plan.entries()) {
        assert_eq!(entry.section_id(), expected.id());
    }
    let accounting = plan.accounting();
    assert_eq!(accounting.total(), plan.entries().len());
    assert_eq!(accounting.total(), accounting.reused() + accounting.invalidated());
}

#[test]
fn clarification_invalidation_closes_transitively_over_prior_entries() {
    let identity = candidate(20, 1, 1);
    let prior = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let request = InvalidationRequest::new(
        state(identity, 11),
        KnowledgeChange::UserClarification,
        vec![section_id(3)],
    )
    .expect("clarification request");

    let plan = plan_invalidation(&prior, &request).expect("dependency-closed plan");
    assert_eq!(
        decision(&plan, 3),
        ReuseDecision::Invalidate(InvalidationReason::UserClarification),
    );
    assert_eq!(
        decision(&plan, 4),
        ReuseDecision::Invalidate(InvalidationReason::DependencyInvalidated),
    );
    assert_eq!(
        decision(&plan, 8),
        ReuseDecision::Invalidate(InvalidationReason::DependencyInvalidated),
    );
    assert_eq!(plan.accounting().reused(), 5);
    assert_eq!(plan.accounting().invalidated(), 3);
}

#[test]
fn late_identity_and_digest_bytes_are_part_of_freshness() {
    let identity = candidate(20, 1, 1);
    let prior = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);

    let mut late_source_bytes = [1; 16];
    late_source_bytes[15] = 2;
    let late_source = KnowledgeSourceId::new(late_source_bytes).expect("late-byte source id");
    let changed_identity_state = CurrentKnowledgeState::new(
        identity,
        vec![
            SourceDigest::new(late_source, digest(11)),
            SourceDigest::new(source_id(2), digest(12)),
        ],
        limits(),
    )
    .expect("current source state");
    let identity_request =
        InvalidationRequest::new(changed_identity_state, KnowledgeChange::SameRevision, Vec::new())
            .expect("same-revision request");
    let identity_plan = plan_invalidation(&prior, &identity_request).expect("identity plan");
    assert_eq!(
        decision(&identity_plan, 1),
        ReuseDecision::Invalidate(InvalidationReason::SourceChanged),
    );

    let mut late_digest_bytes = [11; 32];
    late_digest_bytes[31] = 12;
    let changed_digest_state = CurrentKnowledgeState::new(
        identity,
        vec![
            SourceDigest::new(source_id(1), peritus_types::Sha256Digest::new(late_digest_bytes)),
            SourceDigest::new(source_id(2), digest(12)),
        ],
        limits(),
    )
    .expect("current digest state");
    let digest_request =
        InvalidationRequest::new(changed_digest_state, KnowledgeChange::SameRevision, Vec::new())
            .expect("same-revision request");
    let digest_plan = plan_invalidation(&prior, &digest_request).expect("digest plan");
    assert_eq!(
        decision(&digest_plan, 1),
        ReuseDecision::Invalidate(InvalidationReason::SourceChanged),
    );

    let mut late_section_bytes = [3; 16];
    late_section_bytes[15] = 4;
    let late_section = KnowledgeSectionId::new(late_section_bytes).expect("late-byte section id");
    assert!(prior.section(late_section).is_none());
    let clarification = InvalidationRequest::new(
        state(identity, 11),
        KnowledgeChange::UserClarification,
        vec![late_section],
    )
    .expect("clarification request");
    let error = plan_invalidation(&prior, &clarification).expect_err("unknown late-byte target");
    assert_eq!(error.kind(), KnowledgeErrorKind::InvalidClarificationTarget);
    assert_eq!(error.section_id(), Some(late_section));
}

#[test]
fn future_observations_fail_closed_against_the_supplied_checkpoint() {
    let observed = candidate(20, 1, 5);
    let current = candidate(20, 1, 4);
    let prior = snapshot(observed, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let request =
        InvalidationRequest::new(state(current, 11), KnowledgeChange::SameRevision, Vec::new())
            .expect("earlier checkpoint request");

    let plan = plan_invalidation(&prior, &request).expect("future-observation plan");
    assert!(plan.entries().iter().all(|entry| {
        entry.decision() == ReuseDecision::Invalidate(InvalidationReason::FutureObservation)
    }));
    assert_eq!(plan.accounting().reused(), 0);
    assert_eq!(plan.accounting().invalidated(), prior.sections().len());
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
