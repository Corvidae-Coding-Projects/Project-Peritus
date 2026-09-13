//! Exact snapshot constructor policy and first-error regressions.

mod support;

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CandidateIdentity, KnowledgeBinding, KnowledgeErrorKind, KnowledgeLimits, KnowledgeSection,
    KnowledgeSectionKind, RunKnowledgeSnapshot, SourceDigest,
};
use peritus_types::{RunId, WorkspaceId};
use support::{candidate, digest, limits, section_id, source_id};

fn section(
    identity: CandidateIdentity,
    role: HarnessRole,
    sequence: u64,
    id: u8,
    kind: KnowledgeSectionKind,
    dependencies: &[u8],
) -> KnowledgeSection {
    let binding = KnowledgeBinding::new(
        identity,
        role,
        sequence,
        vec![
            SourceDigest::new(source_id(1), digest(1)),
            SourceDigest::new(source_id(2), digest(2)),
        ],
        limits(),
    )
    .expect("binding with independent bounds");
    KnowledgeSection::new(
        section_id(id),
        kind,
        digest(id),
        binding,
        dependencies.iter().map(|id| section_id(*id)).collect(),
        limits(),
    )
    .expect("section with independent bounds")
}

fn base_sections(
    identity: CandidateIdentity,
    role: HarnessRole,
    sequence: u64,
) -> Vec<KnowledgeSection> {
    vec![
        section(identity, role, sequence, 1, KnowledgeSectionKind::RepositoryInventory, &[]),
        section(identity, role, sequence, 2, KnowledgeSectionKind::RelevantFileMap, &[1]),
        section(
            identity,
            role,
            sequence,
            3,
            KnowledgeSectionKind::LiteralRequirementLedger,
            &[1, 2],
        ),
    ]
}

fn make(
    identity: CandidateIdentity,
    role: HarnessRole,
    sections: Vec<KnowledgeSection>,
    bounds: KnowledgeLimits,
) -> Result<RunKnowledgeSnapshot, peritus_run_knowledge::KnowledgeError> {
    RunKnowledgeSnapshot::new(
        identity,
        role,
        section_id(1),
        section_id(2),
        section_id(3),
        sections,
        bounds,
    )
}

#[test]
fn snapshot_checks_role_minimum_and_maximum_before_section_content() {
    let identity = candidate(20, 1, 3);
    let tiny = KnowledgeLimits::new(1, 1, 1, 1).expect("tiny limits");
    let role = make(identity, HarnessRole::Evaluator, Vec::new(), tiny).expect_err("role first");
    assert_eq!(role.kind(), KnowledgeErrorKind::UnsupportedRole);
    assert_eq!(role.section_id(), None);
    let minimum = make(
        identity,
        HarnessRole::Writer,
        base_sections(identity, HarnessRole::Reviewer, 9).into_iter().take(2).collect(),
        tiny,
    )
    .expect_err("minimum precedes maximum and bad bindings");
    assert_eq!(minimum.kind(), KnowledgeErrorKind::EmptyCollection);
    assert_eq!(minimum.expected(), None);
    let maximum = make(
        identity,
        HarnessRole::Writer,
        base_sections(identity, HarnessRole::Reviewer, 9),
        tiny,
    )
    .expect_err("maximum precedes bad bindings");
    assert_eq!(maximum.kind(), KnowledgeErrorKind::LimitExceeded);
    assert_eq!(maximum.expected(), Some(1));
    assert_eq!(maximum.actual(), Some(3));
    assert_eq!(maximum.section_id(), None);
}

#[test]
fn snapshot_retains_section_order_then_lineage_role_time_dependency_error_priority() {
    let identity = candidate(20, 1, 3);
    let outsider = CandidateIdentity::new(
        RunId::new([43; 16]).expect("other run"),
        WorkspaceId::new([42; 16]).expect("workspace"),
        digest(21),
        1,
        3,
    )
    .expect("other lineage");
    let mut duplicate = base_sections(identity, HarnessRole::Writer, 1);
    duplicate[1] =
        section(outsider, HarnessRole::Fixer, 9, 1, KnowledgeSectionKind::RelevantFileMap, &[]);
    let error = make(identity, HarnessRole::Writer, duplicate, limits()).expect_err("order first");
    assert_eq!(error.kind(), KnowledgeErrorKind::DuplicateValue);
    assert_eq!(error.section_id(), Some(section_id(1)));
    for (bound, role, sequence, dependencies, kind, detail) in [
        (outsider, HarnessRole::Fixer, 9, vec![4], KnowledgeErrorKind::CandidateLineageMismatch, 1),
        (identity, HarnessRole::Fixer, 9, vec![4], KnowledgeErrorKind::RoleMismatch, 1),
        (identity, HarnessRole::Writer, 9, vec![4], KnowledgeErrorKind::FutureKnowledge, 1),
        (identity, HarnessRole::Writer, 1, vec![4], KnowledgeErrorKind::InvalidDependency, 4),
    ] {
        let mut sections = base_sections(identity, HarnessRole::Writer, 1);
        sections[0] = section(
            bound,
            role,
            sequence,
            1,
            KnowledgeSectionKind::RepositoryInventory,
            &dependencies,
        );
        let error = make(identity, HarnessRole::Writer, sections, limits())
            .expect_err("first section failure");
        assert_eq!(error.kind(), kind);
        assert_eq!(error.section_id(), Some(section_id(detail)));
        assert_eq!(error.source_id(), None);
        assert_eq!(error.expected(), None);
        assert_eq!(error.actual(), None);
    }
}

#[test]
fn snapshot_uses_its_own_checkpoint_without_reapplying_nested_allocation_bounds() {
    let current = candidate(20, 1, 3);
    let prior = candidate(10, 9, 1);
    let sections = base_sections(prior, HarnessRole::Reviewer, 3);
    let bounds =
        KnowledgeLimits::new(3, 1, 1, 1).expect("one source and dependency per nested constructor");
    let value = make(current, HarnessRole::Reviewer, sections.clone(), bounds)
        .expect("snapshot only applies its section count bound to already admitted nested values");
    assert_eq!(value.sections(), sections);
    assert_eq!(*value.candidate(), current);
    assert_eq!(*value.sections()[0].binding().candidate(), prior);
    assert_eq!(value.sections()[0].binding().sources().len(), 2);
    assert_eq!(value.sections()[2].dependencies().len(), 2);
    assert_eq!(value.clone(), value);
}

#[test]
fn required_reference_checks_follow_all_section_validation() {
    let identity = candidate(20, 1, 3);
    let mut sections = base_sections(identity, HarnessRole::Writer, 1);
    sections[2] =
        section(identity, HarnessRole::Writer, 1, 3, KnowledgeSectionKind::DesignSection, &[1, 2]);
    let error =
        make(identity, HarnessRole::Writer, sections, limits()).expect_err("wrong required kind");
    assert_eq!(error.kind(), KnowledgeErrorKind::InvalidRequiredSection);
    assert_eq!(error.section_id(), None);
    assert_eq!(error.source_id(), None);
    assert_eq!(error.expected(), None);
    assert_eq!(error.actual(), None);
}
