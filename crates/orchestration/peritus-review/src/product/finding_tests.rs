use super::*;

fn finding(category: ProductFindingCategory, severity: FindingSeverity) -> ProductFinding {
    ProductFinding::new(
        category,
        severity,
        "Observed concern".to_owned(),
        "Specific evidence".to_owned(),
        "src/lib.rs:1".to_owned(),
        "Inspect the cited line".to_owned(),
        "Apply the focused correction".to_owned(),
        1,
    )
    .expect("finding")
}

#[test]
fn advisory_is_nonblocking_in_every_category() {
    for category in [
        ProductFindingCategory::Correctness,
        ProductFindingCategory::RequestedBehavior,
        ProductFindingCategory::BuildCoverage,
        ProductFindingCategory::TestCoverage,
        ProductFindingCategory::Security,
        ProductFindingCategory::Maintainability,
        ProductFindingCategory::Documentation,
    ] {
        assert!(!finding(category, FindingSeverity::Advisory).blocking());
    }
}

#[test]
fn policy_still_blocks_material_findings() {
    assert!(finding(ProductFindingCategory::Correctness, FindingSeverity::Low).blocking());
    assert!(finding(ProductFindingCategory::Maintainability, FindingSeverity::High).blocking());
    assert!(!finding(ProductFindingCategory::Maintainability, FindingSeverity::Medium).blocking());
}

#[test]
fn stable_title_identity_ignores_updated_location_evidence() {
    let first = finding(ProductFindingCategory::Correctness, FindingSeverity::High);
    let moved = ProductFinding::new(
        ProductFindingCategory::Correctness,
        FindingSeverity::High,
        "Observed concern".to_owned(),
        "Specific evidence".to_owned(),
        "src/new_module.rs:9".to_owned(),
        "Inspect the cited line".to_owned(),
        "Apply the focused correction".to_owned(),
        1,
    )
    .expect("moved finding");

    assert_eq!(first.id(), moved.id());
    assert_ne!(first.id(), finding(ProductFindingCategory::Security, FindingSeverity::High).id(),);
}
