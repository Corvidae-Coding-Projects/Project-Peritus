use super::{CANONICAL_POLICY_PACKAGES, policy};
use crate::{
    error::Diagnostic,
    model::ArchitecturePolicy,
    reproducibility::verification_commands::{package_arguments, validate},
};

#[test]
fn canonical_strict_root_inventory_matches_architecture() {
    let policy = policy(CANONICAL_POLICY_PACKAGES);
    assert!(diagnostics(&policy).is_empty());
}

#[test]
fn new_or_reclassified_formal_root_fails_until_strict_commands_exist() {
    for class in ["V", "H"] {
        let policy = policy(&format!(
            r#"
[[packages]]
name = "peritus-approval"
path = "crates/state/peritus-approval"
owner = "B1"
layer = "state"
verification_class = "H"

[[packages]]
name = "peritus-budget"
path = "crates/foundation/peritus-budget"
owner = "B1"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-leases"
path = "crates/state/peritus-leases"
owner = "B1"
layer = "state"
verification_class = "H"

[[packages]]
name = "peritus-policy"
path = "crates/foundation/peritus-policy"
owner = "B1"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-types"
path = "crates/foundation/peritus-types"
owner = "A1"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "missing-strict-root"
path = "crates/missing"
owner = "B0"
layer = "foundation"
verification_class = "{class}"
"#
        ));
        assert!(!diagnostics(&policy).is_empty());
    }
}

#[test]
fn package_inventory_preserves_duplicates_and_order_for_fail_closed_comparison() {
    assert_eq!(
        package_arguments(&[
            "verus",
            "verify",
            "--package",
            "peritus-policy",
            "--package",
            "peritus-policy",
            "--package",
            "peritus-types",
        ]),
        ["peritus-policy", "peritus-policy", "peritus-types"]
    );
}

fn diagnostics(policy: &ArchitecturePolicy) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate(policy, &mut diagnostics);
    diagnostics
}
