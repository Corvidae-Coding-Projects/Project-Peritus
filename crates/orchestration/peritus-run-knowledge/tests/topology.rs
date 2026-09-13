//! Snapshot topology admission regressions.

mod support;

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    KnowledgeBinding, KnowledgeErrorKind, KnowledgeSection, KnowledgeSectionId,
    KnowledgeSectionKind, RunKnowledgeSnapshot, SourceDigest,
};
use support::{candidate, digest, limits, section_id, source_id};

fn section(
    candidate: peritus_run_knowledge::CandidateIdentity,
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
    dependencies: Vec<KnowledgeSectionId>,
) -> KnowledgeSection {
    let binding = KnowledgeBinding::new(
        candidate,
        HarnessRole::Writer,
        candidate.checkpoint_sequence(),
        vec![SourceDigest::new(source_id(1), digest(11))],
        limits(),
    )
    .expect("binding");
    KnowledgeSection::new(id, kind, digest(21), binding, dependencies, limits()).expect("section")
}

fn snapshot_with_second_dependency(
    dependency: KnowledgeSectionId,
) -> Result<RunKnowledgeSnapshot, peritus_run_knowledge::KnowledgeError> {
    let identity = candidate(20, 1, 3);
    let sections = vec![
        section(identity, section_id(1), KnowledgeSectionKind::RepositoryInventory, Vec::new()),
        section(identity, section_id(2), KnowledgeSectionKind::RelevantFileMap, vec![dependency]),
        section(
            identity,
            section_id(3),
            KnowledgeSectionKind::LiteralRequirementLedger,
            Vec::new(),
        ),
    ];
    RunKnowledgeSnapshot::new(
        identity,
        HarnessRole::Writer,
        section_id(1),
        section_id(2),
        section_id(3),
        sections,
        limits(),
    )
}

fn late_id(last: u8) -> KnowledgeSectionId {
    let mut bytes = [9; 16];
    bytes[15] = last;
    KnowledgeSectionId::new(bytes).expect("late-byte identity")
}

#[test]
fn snapshot_rejects_forward_and_missing_dependency_membership() {
    let forward =
        snapshot_with_second_dependency(section_id(3)).expect_err("forward dependency must fail");
    assert_eq!(forward.kind(), KnowledgeErrorKind::InvalidDependency);
    assert_eq!(forward.section_id(), Some(section_id(3)));

    let missing =
        snapshot_with_second_dependency(section_id(4)).expect_err("missing dependency must fail");
    assert_eq!(missing.kind(), KnowledgeErrorKind::InvalidDependency);
    assert_eq!(missing.section_id(), Some(section_id(4)));
}

#[test]
fn snapshot_canonicality_uses_every_identity_byte() {
    let identity = candidate(20, 1, 3);
    let first = late_id(1);
    let second = late_id(2);
    let third = late_id(3);

    let duplicate = vec![
        section(identity, first, KnowledgeSectionKind::RepositoryInventory, Vec::new()),
        section(identity, first, KnowledgeSectionKind::RelevantFileMap, Vec::new()),
        section(identity, third, KnowledgeSectionKind::LiteralRequirementLedger, Vec::new()),
    ];
    let duplicate_error = RunKnowledgeSnapshot::new(
        identity,
        HarnessRole::Writer,
        first,
        first,
        third,
        duplicate,
        limits(),
    )
    .expect_err("adjacent duplicate must fail");
    assert_eq!(duplicate_error.kind(), KnowledgeErrorKind::DuplicateValue);

    let descending = vec![
        section(identity, first, KnowledgeSectionKind::RepositoryInventory, Vec::new()),
        section(identity, third, KnowledgeSectionKind::RelevantFileMap, Vec::new()),
        section(identity, second, KnowledgeSectionKind::LiteralRequirementLedger, Vec::new()),
    ];
    let order_error = RunKnowledgeSnapshot::new(
        identity,
        HarnessRole::Writer,
        first,
        third,
        second,
        descending,
        limits(),
    )
    .expect_err("late-byte descending identity must fail");
    assert_eq!(order_error.kind(), KnowledgeErrorKind::NonCanonicalOrder);
    assert_eq!(order_error.section_id(), Some(second));
}
