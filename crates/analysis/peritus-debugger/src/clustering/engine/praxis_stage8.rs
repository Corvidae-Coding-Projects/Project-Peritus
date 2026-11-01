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
    // Do not call production's classifier: a classification defect must not change the oracle.
    let kind = match member.outcome {
        OutcomeClass::Task(TaskOutcome::Success) => PatternKind::Success,
        OutcomeClass::Task(_) => PatternKind::TaskFailure,
        OutcomeClass::Infrastructure(_) => PatternKind::InfrastructureFailure,
    };
    (kind, member.category, member.analyzer, member.component_kind)
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
        // Check multiple construction histories and repeated application. In particular,
        // don't silently coalesce separate returned groups in the oracle itself.
        for reverse in [false, true] {
            let mut ordered = members.clone();
            if reverse { ordered.reverse(); }
            let mut groups: Vec<_> = ordered.into_iter().map(|item| vec![item]).collect();
            for _ in 0..2 {
                agglomerate(&mut groups);
                prop_assert_eq!(groups.len(), expected.len(), "split or merged equivalence classes");
                let mut actual: BTreeMap<Key, Vec<SubjectId>> = BTreeMap::new();
                for group in &groups {
                    prop_assert!(!group.is_empty());
                    let representative = key(&group[0]);
                    let mut subjects = Vec::new();
                    for item in group {
                        prop_assert_eq!(key(item), representative);
                        let original = members.iter().find(|original| original.subject_id == item.subject_id).unwrap();
                        prop_assert_eq!(item, original, "clustering changed member payload");
                        subjects.push(item.subject_id);
                    }
                    subjects.sort();
                    prop_assert!(actual.insert(representative, subjects).is_none(), "equivalent members remained split");
                }
                prop_assert_eq!(&actual, &expected);
            }
        }
    }
}

#[test]
fn analyzer_signature_is_a_cluster_boundary() {
    let mut groups = vec![vec![member(1, 1, 0, 1)], vec![member(2, 1, 1, 1)]];
    agglomerate(&mut groups);
    assert_eq!(groups.len(), 2);
}

#[test]
fn equivalent_members_must_form_one_group() {
    let mut groups = vec![vec![member(1, 1, 0, 1)], vec![member(2, 1, 0, 1)]];
    agglomerate(&mut groups);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}
