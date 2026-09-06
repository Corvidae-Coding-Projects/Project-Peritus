//! Exact integration-target partition for the durable product runner's bounded CI jobs.

use crate::error::XtaskError;
use crate::model::{CargoMetadata, CargoTarget};
use std::collections::BTreeSet;

pub(super) const PACKAGE: &str = "peritus-product-runner";
pub(super) const RECOVERY_TESTS: &[&str] =
    &["checkpoint_resume", "provider_failover", "role_recovery"];
pub(super) const PRODUCT_TESTS: &[&str] = &["external_effects", "production_composition"];

pub(super) fn validate(cargo: &CargoMetadata) -> Result<(), XtaskError> {
    let package =
        cargo.packages.iter().find(|package| package.name == PACKAGE).ok_or_else(|| {
            XtaskError::metadata("CI runner test plan requires the product runner")
        })?;
    validate_targets(&package.targets)
}

fn validate_targets(targets: &[CargoTarget]) -> Result<(), XtaskError> {
    let expected: BTreeSet<_> = RECOVERY_TESTS.iter().chain(PRODUCT_TESTS).copied().collect();
    let actual: BTreeSet<_> = targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == "test"))
        .map(|target| target.name.as_str())
        .collect();
    if expected.len() != RECOVERY_TESTS.len() + PRODUCT_TESTS.len() || actual != expected {
        return Err(XtaskError::metadata(
            "CI runner integration targets must be assigned exactly once to recovery or product jobs",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PACKAGE, PRODUCT_TESTS, RECOVERY_TESTS, validate_targets};
    use crate::ci_shard::{Operation, cargo_command};
    use crate::model::CargoTarget;
    use std::collections::BTreeSet;
    use std::path::Path;

    fn target(name: &str) -> CargoTarget {
        CargoTarget {
            name: name.to_owned(),
            kind: vec!["test".to_owned()],
            crate_types: vec!["bin".to_owned()],
            src_path: format!("tests/{name}.rs").into(),
        }
    }

    #[test]
    fn integration_partition_rejects_missing_and_unassigned_targets() {
        let mut targets: Vec<_> =
            RECOVERY_TESTS.iter().chain(PRODUCT_TESTS).map(|name| target(name)).collect();
        assert!(validate_targets(&targets).is_ok());
        targets.push(target("future_integration"));
        assert!(validate_targets(&targets).is_err());
        targets.pop();
        targets.pop();
        assert!(validate_targets(&targets).is_err());
    }

    fn arguments(operation: Operation) -> Vec<String> {
        cargo_command(Path::new("."), operation, &[PACKAGE])
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn runner_test_commands_cover_each_target_kind_and_integration_once() {
        let regular = arguments(Operation::Test);
        for selector in ["--lib", "--bins", "--examples", "--benches", "--all-features"] {
            assert!(regular.iter().any(|argument| argument == selector));
        }
        assert!(
            !regular.iter().any(|argument| matches!(
                argument.as_str(),
                "--all-targets" | "--tests" | "--test"
            ))
        );
        let mut selected = BTreeSet::new();
        for (name, operation, expected) in [
            ("test-runner-recovery", Operation::TestRunnerRecovery, RECOVERY_TESTS),
            ("test-runner-product", Operation::TestRunnerProduct, PRODUCT_TESTS),
        ] {
            assert_eq!(Operation::parse(name), Some(operation));
            let arguments = arguments(operation);
            assert!(arguments.iter().any(|argument| argument == "--test-threads=1"));
            assert!(!arguments.iter().any(|argument| matches!(
                argument.as_str(),
                "--all-targets" | "--tests" | "--skip"
            )));
            let targets: Vec<_> = arguments
                .windows(2)
                .filter(|pair| pair[0] == "--test")
                .map(|pair| pair[1].as_str())
                .collect();
            assert_eq!(targets, expected);
            for target in targets {
                assert!(selected.insert(target.to_owned()), "integration target repeated");
            }
        }
        assert_eq!(selected.len(), RECOVERY_TESTS.len() + PRODUCT_TESTS.len());
    }
}
