//! Public delta-packet admission, identity, classification, and cloning boundaries.

mod support;

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CurrentKnowledgeState, DeltaDelivery, DeltaPacket, InvalidationRequest, KnowledgeBinding,
    KnowledgeChange, KnowledgeError, KnowledgeErrorKind, KnowledgeSection, KnowledgeSectionId,
    SourceDigest, plan_delta_packet, plan_invalidation,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use support::{FixtureRevision, candidate, digest, limits, section_id, snapshot, source_id, state};

fn delivery(packet: &DeltaPacket, id: KnowledgeSectionId) -> DeltaDelivery {
    packet
        .entries()
        .iter()
        .find(|entry| entry.section_id() == id)
        .expect("packet section")
        .delivery()
}

fn assert_plain_error(error: KnowledgeError, kind: KnowledgeErrorKind) {
    assert_eq!(error.kind(), kind);
    assert_eq!(error.section_id(), None);
    assert_eq!(error.source_id(), None);
    assert_eq!(error.expected(), None);
    assert_eq!(error.actual(), None);
}

fn assert_section_error(
    error: KnowledgeError,
    kind: KnowledgeErrorKind,
    section_id: KnowledgeSectionId,
) {
    assert_eq!(error.kind(), kind);
    assert_eq!(error.section_id(), Some(section_id));
    assert_eq!(error.source_id(), None);
    assert_eq!(error.expected(), None);
    assert_eq!(error.actual(), None);
}

fn replace_section(
    base: &peritus_run_knowledge::RunKnowledgeSnapshot,
    target: KnowledgeSectionId,
    replacement_digest: Sha256Digest,
    replacement_sources: Option<&[SourceDigest]>,
    replacement_dependencies: Option<&[KnowledgeSectionId]>,
) -> peritus_run_knowledge::RunKnowledgeSnapshot {
    let sections = base
        .sections()
        .iter()
        .map(|section| {
            if section.id() != target {
                return section.clone();
            }
            let binding = KnowledgeBinding::new(
                *section.binding().candidate(),
                section.binding().role(),
                section.binding().creation_sequence(),
                replacement_sources
                    .map_or_else(|| section.binding().sources().to_vec(), <[SourceDigest]>::to_vec),
                limits(),
            )
            .expect("replacement binding");
            KnowledgeSection::new(
                section.id(),
                section.kind(),
                replacement_digest,
                binding,
                replacement_dependencies.map_or_else(
                    || section.dependencies().to_vec(),
                    <[KnowledgeSectionId]>::to_vec,
                ),
                limits(),
            )
            .expect("replacement section")
        })
        .collect();
    peritus_run_knowledge::RunKnowledgeSnapshot::new(
        *base.candidate(),
        base.role(),
        base.repository_inventory(),
        base.relevant_file_map(),
        base.requirement_ledger(),
        sections,
        limits(),
    )
    .expect("replacement snapshot")
}

fn late_digest_candidate() -> CandidateIdentity {
    let mut bytes = [20; 32];
    bytes[31] = 21;
    CandidateIdentity::new(
        RunId::new([41; 16]).expect("run id"),
        WorkspaceId::new([42; 16]).expect("workspace id"),
        Sha256Digest::new(bytes),
        1,
        1,
    )
    .expect("late-byte candidate")
}

#[test]
fn delta_admission_preserves_error_precedence() {
    let identity = candidate(20, 1, 1);
    let writer = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let reviewer = snapshot(identity, HarnessRole::Reviewer, 11, FixtureRevision::Baseline);
    let invalid_target = section_id(9);
    let wrong_candidate_request = InvalidationRequest::new(
        state(late_digest_candidate(), 13),
        KnowledgeChange::UserClarification,
        vec![invalid_target],
    )
    .expect("structurally valid request");

    assert_plain_error(
        plan_delta_packet(&reviewer, &writer, &wrong_candidate_request)
            .expect_err("role mismatch has priority"),
        KnowledgeErrorKind::RoleMismatch,
    );
    assert_plain_error(
        plan_delta_packet(&writer, &writer, &wrong_candidate_request)
            .expect_err("candidate mismatch precedes target validation"),
        KnowledgeErrorKind::CurrentSnapshotStale,
    );

    let source_a = SourceDigest::new(source_id(1), digest(11));
    let incomplete_current_state = CurrentKnowledgeState::new(identity, vec![source_a], limits())
        .expect("incomplete but canonical current state");
    let stale_current_request = InvalidationRequest::new(
        incomplete_current_state,
        KnowledgeChange::UserClarification,
        vec![section_id(3), invalid_target],
    )
    .expect("structurally valid stale request");
    assert_section_error(
        plan_delta_packet(&writer, &writer, &stale_current_request)
            .expect_err("current freshness precedes prior target validation"),
        KnowledgeErrorKind::CurrentSnapshotStale,
        section_id(3),
    );

    let invalid_target_request = InvalidationRequest::new(
        state(identity, 11),
        KnowledgeChange::UserClarification,
        vec![section_id(3), invalid_target, section_id(10)],
    )
    .expect("structurally valid target request");
    assert_section_error(
        plan_invalidation(&writer, &invalid_target_request)
            .expect_err("planner reports the first invalid target"),
        KnowledgeErrorKind::InvalidClarificationTarget,
        invalid_target,
    );
    assert_section_error(
        plan_delta_packet(&writer, &writer, &invalid_target_request)
            .expect_err("invalid prior target"),
        KnowledgeErrorKind::InvalidClarificationTarget,
        invalid_target,
    );
}

#[test]
fn exact_late_bytes_and_dependency_material_drive_changed_facts() {
    let identity = candidate(20, 1, 1);
    let previous = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);

    let mut late_section_digest = [3; 32];
    late_section_digest[31] = 4;
    let changed_digest = replace_section(
        &previous,
        section_id(3),
        Sha256Digest::new(late_section_digest),
        None,
        None,
    );
    let request =
        InvalidationRequest::new(state(identity, 11), KnowledgeChange::SameRevision, Vec::new())
            .expect("same revision request");
    let packet = plan_delta_packet(&previous, &changed_digest, &request).expect("digest delta");
    assert_eq!(delivery(&packet, section_id(3)), DeltaDelivery::ChangedFact);
    assert_eq!(delivery(&packet, section_id(1)), DeltaDelivery::CurrentReference);

    let changed_dependency =
        replace_section(&previous, section_id(4), digest(4), None, Some(&[section_id(1)]));
    let packet =
        plan_delta_packet(&previous, &changed_dependency, &request).expect("dependency delta");
    assert_eq!(delivery(&packet, section_id(4)), DeltaDelivery::ChangedFact);
}

#[test]
fn exact_source_identity_and_authority_control_delivery() {
    let identity = candidate(20, 1, 1);
    let previous = snapshot(identity, HarnessRole::Writer, 11, FixtureRevision::Baseline);
    let mut late_source_bytes = [1; 16];
    late_source_bytes[15] = 2;
    let late_source = SourceDigest::new(
        peritus_run_knowledge::KnowledgeSourceId::new(late_source_bytes)
            .expect("late-byte source id"),
        digest(11),
    );
    let current = replace_section(&previous, section_id(1), digest(1), Some(&[late_source]), None);
    let request_state = CurrentKnowledgeState::new(
        identity,
        vec![
            SourceDigest::new(source_id(1), digest(11)),
            late_source,
            SourceDigest::new(source_id(2), digest(12)),
        ],
        limits(),
    )
    .expect("current source state");
    let request =
        InvalidationRequest::new(request_state, KnowledgeChange::SameRevision, Vec::new())
            .expect("same revision request");
    let packet = plan_delta_packet(&previous, &current, &request).expect("source identity delta");

    assert_eq!(delivery(&packet, section_id(1)), DeltaDelivery::ChangedFact);
    assert_eq!(delivery(&packet, section_id(5)), DeltaDelivery::Navigation);
    assert_eq!(delivery(&packet, section_id(8)), DeltaDelivery::Navigation);
}

#[test]
fn packet_getters_accounting_and_clone_preserve_complete_public_result() {
    let identity = candidate(20, 1, 1);
    let previous = snapshot(identity, HarnessRole::Fixer, 11, FixtureRevision::Baseline);
    let current = snapshot(identity, HarnessRole::Fixer, 13, FixtureRevision::SourceChanged);
    let request =
        InvalidationRequest::new(state(identity, 13), KnowledgeChange::SourceChanged, Vec::new())
            .expect("source request");
    let packet = plan_delta_packet(&previous, &current, &request).expect("delta packet");
    let cloned = packet.clone();

    assert_eq!(cloned, packet);
    assert_eq!(packet.role(), HarnessRole::Fixer);
    assert_eq!(packet.candidate(), current.candidate());
    assert_eq!(packet.entries().len(), current.sections().len());
    let accounting = packet.accounting();
    assert_eq!(
        accounting.changed_facts()
            + accounting.current_references()
            + accounting.navigation_sections(),
        packet.entries().len(),
    );
    assert_eq!(accounting.invalidated_prior_sections(), 6);
}
