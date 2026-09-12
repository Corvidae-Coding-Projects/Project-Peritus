//! Evidence-backed exclusions for host capabilities unavailable to an unrelated package test.

use super::super::write_json;
use crate::error::XtaskError;
use serde_json::json;
use std::path::Path;
use std::process::Command;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};

const LIFECYCLE_TEST_FILTER: &str = "product_run::";

#[cfg(unix)]
pub(super) fn configure(
    campaign: &mut Command,
    repository: &Path,
    evidence: &Path,
    restrict_to_lifecycle_tests: bool,
) -> Result<(), XtaskError> {
    if !restrict_to_lifecycle_tests {
        return write_json(
            &evidence.join("mutation-environment.json"),
            &json!({"unix_socket_bind": "not_required", "excluded_tests": []}),
        );
    }

    append_lifecycle_filter(campaign);
    configure_unix(repository, evidence)
}

#[cfg(not(unix))]
pub(super) fn configure(
    campaign: &mut Command,
    _repository: &Path,
    evidence: &Path,
    restrict_to_lifecycle_tests: bool,
) -> Result<(), XtaskError> {
    if restrict_to_lifecycle_tests {
        append_lifecycle_filter(campaign);
    }
    write_json(
        &evidence.join("mutation-environment.json"),
        &json!({
            "unix_socket_bind": "not_applicable",
            "cargo_test_filter": restrict_to_lifecycle_tests.then_some(LIFECYCLE_TEST_FILTER),
            "excluded_tests": [],
        }),
    )
}

#[cfg(unix)]
fn configure_unix(_repository: &Path, evidence: &Path) -> Result<(), XtaskError> {
    use std::os::unix::net::UnixListener;

    static NEXT_PROBE: AtomicU64 = AtomicU64::new(0);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "p-{}-{}-{timestamp:x}.sock",
        std::process::id(),
        NEXT_PROBE.fetch_add(1, Ordering::Relaxed)
    ));
    match UnixListener::bind(&path) {
        Ok(listener) => {
            drop(listener);
            std::fs::remove_file(&path)
                .map_err(|error| XtaskError::io("remove Unix socket probe", &path, error))?;
            write_json(
                &evidence.join("mutation-environment.json"),
                &json!({
                    "unix_socket_bind": "supported",
                    "cargo_test_filter": LIFECYCLE_TEST_FILTER,
                    "reason": "the cancellation mutation slice executes its owning product-run namespace within the per-mutant timeout",
                    "excluded_tests": [],
                }),
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => write_json(
            &evidence.join("mutation-environment.json"),
            &json!({
                "unix_socket_bind": "permission_denied",
                "cargo_test_filter": LIFECYCLE_TEST_FILTER,
                "reason": "host sandbox denied a direct AF_UNIX bind probe; the target lifecycle namespace remains enabled while socket-dependent integration tests are filtered",
            }),
        ),
        Err(error) => Err(XtaskError::io("probe Unix socket capability", &path, error)),
    }
}

fn append_lifecycle_filter(campaign: &mut Command) {
    // cargo-mutants consumes the separator and forwards the filter to Cargo.
    campaign.args(["--", LIFECYCLE_TEST_FILTER]);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_filter_reaches_cargo_test() {
        let mut command = Command::new("cargo");
        command.arg("mutants");
        append_lifecycle_filter(&mut command);
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments, ["mutants", "--", LIFECYCLE_TEST_FILTER]);
    }

    #[test]
    fn cancellation_campaign_always_keeps_the_owner_filter() {
        let root = std::env::temp_dir().join(format!(
            "peritus-mutation-environment-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let repository = root.join("repository");
        let evidence = root.join("evidence");
        std::fs::create_dir_all(&repository).expect("repository");
        std::fs::create_dir_all(&evidence).expect("evidence");
        let mut command = Command::new("cargo");
        command.arg("mutants");

        configure(&mut command, &repository, &evidence, true)
            .expect("configure cancellation campaign");

        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments, ["mutants", "--", LIFECYCLE_TEST_FILTER]);
        let report: serde_json::Value = serde_json::from_slice(
            &std::fs::read(evidence.join("mutation-environment.json"))
                .expect("environment evidence"),
        )
        .expect("environment JSON");
        assert_eq!(report["cargo_test_filter"], LIFECYCLE_TEST_FILTER);
        std::fs::remove_dir_all(root).expect("remove fixture");
    }
}
