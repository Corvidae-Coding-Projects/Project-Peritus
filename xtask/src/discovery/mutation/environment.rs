//! Evidence-backed exclusions for host capabilities unavailable to an unrelated package test.

use super::super::write_json;
use crate::error::XtaskError;
use serde_json::json;
use std::path::Path;
use std::process::Command;

#[cfg(unix)]
const SOCKET_TEST: &str =
    "ipc::server::tests::delayed_accept_remains_cancellable_without_consuming_the_queued_client";

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
            campaign.args(["--", "--skip", SOCKET_TEST]);
            write_json(
                &evidence.join("mutation-environment.json"),
                &json!({
                    "unix_socket_bind": "permission_denied",
                    "excluded_tests": [SOCKET_TEST],
                    "reason": "host sandbox denied a direct AF_UNIX bind probe; target lifecycle tests remain enabled",
                }),
            )
        }
        Err(error) => Err(XtaskError::io("probe Unix socket capability", &path, error)),
    }
}
