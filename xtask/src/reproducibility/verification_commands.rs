use super::verus_commands::{VERUS_STRICT_BUILD_ARGS, VERUS_STRICT_VERIFY_ARGS};
use crate::error::Diagnostic;
use crate::model::ArchitecturePolicy;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn validate(policy: &ArchitecturePolicy, diagnostics: &mut Vec<Diagnostic>) {
    let required: BTreeSet<_> = policy
        .packages
        .iter()
        .filter(|package| matches!(package.verification_class.as_str(), "V" | "H"))
        .map(|package| package.name.as_str())
        .collect();
    let verify = package_arguments(VERUS_STRICT_VERIFY_ARGS);
    let build = package_arguments(VERUS_STRICT_BUILD_ARGS);
    let expected: Vec<_> = required.iter().copied().collect();

    if verify != expected || build != expected {
        diagnostics.push(Diagnostic::at(
            Path::new("justfile"),
            "V/H no-cheating command coverage differs from the architecture registry",
            format!(
                "configure one exact no-cheating verify and release-build command for every V/H package; required: {}",
                required.iter().copied().collect::<Vec<_>>().join(", ")
            ),
        ));
    }
}

fn package_arguments<'arguments>(arguments: &'arguments [&'arguments str]) -> Vec<&'arguments str> {
    arguments.windows(2).filter_map(|pair| (pair[0] == "--package").then_some(pair[1])).collect()
}

#[cfg(test)]
mod tests {
    use super::{package_arguments, validate};
    use crate::error::Diagnostic;
    use crate::model::ArchitecturePolicy;

    fn policy(packages: &str) -> ArchitecturePolicy {
        toml::from_str(&format!(
            r#"
schema = 3
soft_source_lines = 400
hard_source_lines = 700
root_module_lines = 80
required_license = "MIT"
ignored_directories = []
forbidden_module_names = []
trusted_source_roots = []
source_exceptions = []
layers = []
verification_classes = []
forbidden_dependencies = []
controlled_source_roots = []
{packages}
"#
        ))
        .expect("verification command policy fixture must parse")
    }

    #[test]
    fn canonical_strict_root_inventory_matches_architecture() {
        let policy = policy(
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
name = "peritus-kernel"
path = "crates/foundation/peritus-kernel"
owner = "B0"
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
name = "peritus-quality-policy"
path = "crates/foundation/peritus-quality-policy"
owner = "B2"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-spec"
path = "crates/foundation/peritus-spec"
owner = "B2"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-types"
path = "crates/foundation/peritus-types"
owner = "A1"
layer = "foundation"
verification_class = "V"

[[packages]]
name = "peritus-tcb"
path = "crates/foundation/peritus-tcb"
owner = "A1"
layer = "foundation"
verification_class = "T"
"#,
        );
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
}
