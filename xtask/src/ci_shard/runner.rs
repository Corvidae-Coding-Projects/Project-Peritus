//! Exhaustive runner test partitions within the retained hosted-job deadline.

use super::Operation;
use crate::error::XtaskError;
use crate::model::{CargoMetadata, CargoTarget};
use std::collections::BTreeSet;
use std::{path::Path, process::Command};

pub(super) const PACKAGE: &str = "peritus-product-runner";
pub(super) const RECOVERY_TESTS: &[&str] =
    &["checkpoint_resume", "in_place_recovery", "provider_failover", "role_recovery"];
pub(super) const PRODUCT_TESTS: &[&str] = &["external_effects", "production_composition"];

const CANDIDATE: &str = "candidate::";
const LOCAL_CONTEXT: &str = "local_context::";

pub(super) fn library_filters(operation: Operation, windows: bool) -> Vec<&'static str> {
    if !windows {
        return Vec::new();
    }
    match operation {
        Operation::Test => vec!["--skip", CANDIDATE, "--skip", LOCAL_CONTEXT],
        Operation::TestRunnerRecovery => vec![CANDIDATE, "--skip", LOCAL_CONTEXT],
        Operation::TestRunnerProduct => vec![LOCAL_CONTEXT],
        _ => Vec::new(),
    }
}

fn library_command(root: &Path, operation: Operation, windows: bool) -> Option<Command> {
    if !windows
        || !matches!(operation, Operation::TestRunnerRecovery | Operation::TestRunnerProduct)
    {
        return None;
    }
    let mut command = Command::new("cargo");
    command.current_dir(root).args([
        "test",
        "--locked",
        "--package",
        PACKAGE,
        "--lib",
        "--all-features",
        "--",
        "--test-threads=1",
    ]);
    command.args(library_filters(operation, windows));
    Some(command)
}

pub(super) fn run_library_partition(
    root: &Path,
    operation: Operation,
    windows: bool,
) -> Result<(), XtaskError> {
    let Some(mut command) = library_command(root, operation, windows) else { return Ok(()) };
    let status = command
        .status()
        .map_err(|error| XtaskError::io("execute runner library partition from", root, error))?;
    if !status.success() {
        return Err(XtaskError::metadata(format!(
            "runner library partition {operation:?} failed with {status}"
        )));
    }
    Ok(())
}

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
    use super::{
        PACKAGE, PRODUCT_TESTS, RECOVERY_TESTS, library_command, library_filters, validate_targets,
    };
    use crate::ci_shard::{Operation, cargo_command};
    use crate::model::CargoTarget;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn windows_library_namespaces_execute_once_across_existing_jobs() {
        let operations =
            [Operation::Test, Operation::TestRunnerRecovery, Operation::TestRunnerProduct];
        for name in [
            "candidate::managed::tests::restore",
            "local_context::tests::capacity",
            "local_context::candidate::future_test",
            "developer_tools::tests::future_test",
            "future_module::test",
        ] {
            let owners = operations
                .iter()
                .filter(|operation| matches_filters(name, &library_filters(**operation, true)))
                .count();
            assert_eq!(owners, 1, "test must execute exactly once: {name}");
        }
        assert!(library_filters(Operation::Test, false).is_empty());
        assert!(library_command(Path::new("."), Operation::TestRunnerRecovery, false).is_none());
        assert!(library_command(Path::new("."), Operation::TestRunnerProduct, false).is_none());
        assert!(library_command(Path::new("."), Operation::Test, true).is_none());
        for operation in [Operation::TestRunnerRecovery, Operation::TestRunnerProduct] {
            let command = library_command(Path::new("."), operation, true).unwrap();
            let arguments =
                command.get_args().map(|value| value.to_string_lossy()).collect::<Vec<_>>();
            for required in ["--locked", "--lib", "--all-features", "--test-threads=1"] {
                assert!(arguments.iter().any(|argument| argument == required));
            }
            assert!(!arguments.iter().any(|argument| matches!(
                argument.as_ref(),
                "--ignored" | "--test" | "--all-targets"
            )));
        }
    }

    fn matches_filters(name: &str, filters: &[&str]) -> bool {
        let mut positive = None;
        let mut filters = filters.iter();
        while let Some(filter) = filters.next() {
            if *filter == "--skip" {
                if name.contains(filters.next().unwrap()) {
                    return false;
                }
            } else {
                positive = Some(*filter);
            }
        }
        positive.is_none_or(|filter| name.contains(filter))
    }

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
