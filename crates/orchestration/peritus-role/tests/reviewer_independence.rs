//! Checks that C6 projects B2 reviewer-independence requirements without weakening them.

use peritus_role::ReviewIndependenceView;
use peritus_spec::ReviewerIndependence;

#[test]
fn projection_preserves_every_contract_fact_and_requires_fresh_context() {
    let requirements = ReviewerIndependence::new(true, false, true, false, true, true);
    let view = ReviewIndependenceView::from_contract(requirements);
    assert!(view.distinct_reviewers());
    assert!(!view.independent_from_producer());
    assert!(view.distinct_contexts());
    assert!(!view.distinct_model_families());
    assert!(view.distinct_providers());
    assert!(view.no_shared_ancestry());
    assert!(view.fresh_context());
}
