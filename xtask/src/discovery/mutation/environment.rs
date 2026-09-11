//! Evidence-backed exclusions for host capabilities unavailable to an unrelated package test.

use super::super::write_json;
use crate::error::XtaskError;
use serde_json::json;
use std::path::Path;
use std::process::Command;

#[cfg(unix)]
const LIFECYCLE_TEST_FILTER: &str = "product_run::";

pub(super) fn configure(
    campaign: &mut Command,
    repository: &Path,
    evidence: &Path,
) -> Result<(), XtaskError> {
    #[cfg(unix)]
    return configure_unix(campaign, repository, evidence);

    #[cfg(windows)]
    write_json(
        &evidence.join("mutation-environment.json"),
        &json!({"unix_socket_bind": "not_applicable", "excluded_tests": []}),
    )
}

#[cfg(unix)]
fn configure_unix(
    campaign: &mut Command,
    repository: &Path,
    evidence: &Path,
) -> Result<(), XtaskError> {
    use std::os::unix::net::UnixListener;

    let path = repository.join(".peritus-discovery-socket-probe");
    match UnixListener::bind(&path) {
        Ok(listener) => {
            drop(listener);
            std::fs::remove_file(&path)
                .map_err(|error| XtaskError::io("remove Unix socket probe", &path, error))?;
            write_json(
                &evidence.join("mutation-environment.json"),
                &json!({"unix_socket_bind": "supported", "excluded_tests": []}),
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            append_lifecycle_filter(campaign);
            write_json(
                &evidence.join("mutation-environment.json"),
                &json!({
                    "unix_socket_bind": "permission_denied",
                    "cargo_test_filter": LIFECYCLE_TEST_FILTER,
                    "reason": "host sandbox denied a direct AF_UNIX bind probe; the target lifecycle namespace remains enabled while socket-dependent integration tests are filtered",
                }),
            )
        }
        Err(error) => Err(XtaskError::io("probe Unix socket capability", &path, error)),
    }
}

#[cfg(unix)]
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
}
