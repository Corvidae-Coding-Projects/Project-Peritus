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
    let verify = package_argument(VERUS_STRICT_VERIFY_ARGS);
    let build = package_argument(VERUS_STRICT_BUILD_ARGS);
    let configured: BTreeSet<_> = verify.into_iter().collect();

    if configured != required || verify != build {
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

fn package_argument<'arguments>(
    arguments: &'arguments [&'arguments str],
) -> Option<&'arguments str> {
    arguments.windows(2).find_map(|pair| (pair[0] == "--package").then_some(pair[1]))
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::error::Diagnostic;
    use crate::model::ArchitecturePolicy;

    fn policy(packages: &str) -> ArchitecturePolicy {
        toml::from_str(&format!(
            r#"
schema = 2
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

    fn diagnostics(policy: &ArchitecturePolicy) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        validate(policy, &mut diagnostics);
        diagnostics
    }
}
