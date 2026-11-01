use super::*;
use crate::{InfrastructureOutcome, TaskOutcome};
use peritus_harness::domain::RevisionDigest;
use peritus_types::{HarnessId, Sha256Digest};
use proptest::prelude::*;

type Key = (PatternKind, Option<FailureCategory>, AnalyzerSignature, Option<ComponentKind>);

fn member(index: u8, class: u8, signature: u8, component: u8) -> PatternMember {
    let (outcome, category) = match class {
        0 => (OutcomeClass::Task(TaskOutcome::Success), None),
        1 => (
            OutcomeClass::Task(TaskOutcome::Blocked),
            Some(FailureCategory::SpecificationAmbiguity),
        ),
        2 => (
            OutcomeClass::Infrastructure(InfrastructureOutcome::ProviderFailure),
            Some(FailureCategory::ProviderTransport),
        ),
        _ => (
            OutcomeClass::Infrastructure(InfrastructureOutcome::ToolFailure),
            Some(FailureCategory::ToolExecution),
        ),
    };
    let analyzer = match signature % 3 {
        0 => AnalyzerSignature::TerminalOutcome,
        1 => AnalyzerSignature::RepeatedDiagnostic,
        _ => AnalyzerSignature::SuccessPath,
    };
    let component_kind = match component % 3 {
        0 => None,
        1 => Some(ComponentKind::ToolImplementation),
        _ => Some(ComponentKind::RolePrompt),
    };
    let base = PatternFingerprint::combined(&[]);
    let fingerprint = PatternFingerprint::combined(&vec![base; usize::from(index)]);
    PatternMember {
        subject_id: SubjectId::new([index; 16]).unwrap(),
        outcome,
        category,
        analyzer,
        environment_id: EnvironmentId::new([1; 16]).unwrap(),
        harness_revision: HarnessRevisionIdentity::new(
            HarnessId::new([1; 16]).unwrap(),
            RevisionNumber::first(),
            RevisionDigest::new(Sha256Digest::new([1; 32])),
        ),
        workspace_revision: RevisionNumber::first(),
        provider_profile_id: ProviderProfileId::new([1; 16]).unwrap(),
        component_kind,
        citations: Vec::new(),
        fingerprint,
    }
}

fn key(member: &PatternMember) -> Key {
    (pattern_kind(member.outcome), member.category, member.analyzer, member.component_kind)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn agglomeration_preserves_independent_equivalence_classes(
        input in proptest::collection::vec((0_u8..4, 0_u8..3, 0_u8..3), 1..50)
    ) {
        let members: Vec<_> = input.into_iter().enumerate().map(|(i, (class, signature, component))|
            member(u8::try_from(i + 1).unwrap(), class, signature, component)
        ).collect();
        let mut expected: BTreeMap<Key, Vec<SubjectId>> = BTreeMap::new();
        for item in &members {
            expected.entry(key(item)).or_default().push(item.subject_id);
        }
        for subjects in expected.values_mut() { subjects.sort(); }
        let mut groups: Vec<_> = members.into_iter().map(|item| vec![item]).collect();
        agglomerate(&mut groups);
        let mut actual: BTreeMap<Key, Vec<SubjectId>> = BTreeMap::new();
        for group in groups {
            let representative = key(&group[0]);
            for item in group {
                prop_assert_eq!(key(&item), representative);
                actual.entry(representative).or_default().push(item.subject_id);
            }
        }
        for subjects in actual.values_mut() { subjects.sort(); }
        prop_assert_eq!(actual, expected);
    }
}

#[test]
fn analyzer_signature_is_a_cluster_boundary() {
    let mut groups = vec![vec![member(1, 1, 0, 1)], vec![member(2, 1, 1, 1)]];
    agglomerate(&mut groups);
    assert_eq!(groups.len(), 2);
}
