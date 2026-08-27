//! Closed production criterion and evidence catalog contracts.

mod support;

use peritus_release_policy::{AcceptanceCriterion, PRODUCTION_CRITERIA, REQUIRED_EVIDENCE};

#[test]
fn production_catalog_is_exactly_twenty_five_stable_items() {
    assert_eq!(PRODUCTION_CRITERIA.len(), 25);
    for (index, definition) in PRODUCTION_CRITERIA.iter().enumerate() {
        assert_eq!(usize::from(definition.criterion().stable_id()), index + 1);
        assert!(!definition.title().is_empty());
        assert!(!definition.statement().is_empty());
    }
    assert_eq!(PRODUCTION_CRITERIA[0].criterion(), AcceptanceCriterion::CleanTierOneSuite);
    assert_eq!(PRODUCTION_CRITERIA[24].criterion(), AcceptanceCriterion::NoReleaseDebt);
}

#[test]
fn every_criterion_has_at_least_one_required_artifact() {
    for definition in PRODUCTION_CRITERIA {
        assert!(
            REQUIRED_EVIDENCE
                .iter()
                .any(|requirement| requirement.criterion() == definition.criterion())
        );
    }
}

#[test]
fn evidence_catalog_has_stable_unique_ids_and_sources() {
    assert_eq!(REQUIRED_EVIDENCE.len(), 44);
    for (index, requirement) in REQUIRED_EVIDENCE.iter().enumerate() {
        assert_eq!(usize::from(requirement.stable_id()), index + 1);
        assert_eq!(requirement.source_kind(), REQUIRED_EVIDENCE[index].source_kind());
    }
}
