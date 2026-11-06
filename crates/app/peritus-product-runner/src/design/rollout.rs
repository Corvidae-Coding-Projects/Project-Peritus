//! Internal, fail-closed rollout control for new domain-expansion publication.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const CONTROL_FILE: &str = ".domain-expansion-rollout-v1";
const RECOVERY_ONLY: &[u8] = b"recovery-only-v1\n";

/// Rejects creation of a new expansion while the internal recovery marker is active.
///
/// Exact ready-receipt reuse and deterministic artifact work are checked before this boundary.
/// Active durable runs restore their accepted design without entering this path. The marker does
/// not delete or reinterpret source, manifest, expanded-design, or receipt artifacts.
pub(super) fn require_new_expansion(trace_path: &Path) -> Result<(), ProductRunnerError> {
    let path = control_path(trace_path)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(rollout(error.to_string())),
    };
    if !metadata.file_type().is_file() {
        return Err(rollout("internal rollout control is not a regular file"));
    }
    let bytes = fs::read(&path).map_err(|error| rollout(error.to_string()))?;
    if bytes != RECOVERY_ONLY {
        return Err(rollout("internal rollout control has unsupported contents"));
    }
    Err(rollout(
        "new domain expansion creation is disabled by the internal recovery control; retained artifacts and active runs remain readable",
    ))
}

fn control_path(trace_path: &Path) -> Result<PathBuf, ProductRunnerError> {
    trace_path
        .parent()
        .map(|parent| parent.join(CONTROL_FILE))
        .ok_or_else(|| rollout("product-run trace has no internal state directory"))
}

fn rollout(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "apply internal domain-expansion rollout policy",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_runs_default_to_domain_expansion() {
        let directory = tempfile::tempdir().expect("temporary rollout state");
        let trace = directory.path().join("run.trace");
        require_new_expansion(&trace).expect("absent internal marker defaults on");
    }

    #[test]
    fn recovery_marker_blocks_only_new_creation_and_preserves_artifacts() {
        let directory = tempfile::tempdir().expect("temporary rollout state");
        let trace = directory.path().join("run.trace");
        let source = trace.with_extension("design-source.json");
        let expansion = trace.with_extension("design-expansion.json");
        let design = trace.with_extension("design.md");
        fs::write(&source, b"source-before").expect("source fixture");
        fs::write(&expansion, b"manifest-before").expect("manifest fixture");
        fs::write(&design, b"design-before").expect("design fixture");
        fs::write(control_path(&trace).expect("control path"), RECOVERY_ONLY)
            .expect("activate recovery-only rollout");

        let error = require_new_expansion(&trace).expect_err("new expansion must be blocked");
        assert_eq!(error.kind(), ProductRunnerErrorKind::InvalidPrecondition);
        assert!(error.detail().contains("active runs remain readable"));
        assert_eq!(fs::read(source).expect("source retained"), b"source-before");
        assert_eq!(fs::read(expansion).expect("manifest retained"), b"manifest-before");
        assert_eq!(fs::read(design).expect("design retained"), b"design-before");
    }

    #[test]
    fn malformed_or_non_file_controls_fail_closed() {
        let directory = tempfile::tempdir().expect("temporary rollout state");
        let trace = directory.path().join("run.trace");
        let control = control_path(&trace).expect("control path");
        fs::write(&control, b"disabled\n").expect("malformed marker");
        assert!(require_new_expansion(&trace).is_err());
        fs::remove_file(&control).expect("remove malformed marker");
        fs::create_dir(&control).expect("non-file marker");
        assert!(require_new_expansion(&trace).is_err());
    }

    #[test]
    fn control_lookup_errors_other_than_absence_fail_closed() {
        let directory = tempfile::tempdir().expect("temporary rollout state");
        let state_file = directory.path().join("not-a-directory");
        fs::write(&state_file, b"occupied").expect("non-directory state fixture");
        let trace = state_file.join("run.trace");

        let error =
            require_new_expansion(&trace).expect_err("lookup error must not enable rollout");
        assert_eq!(error.kind(), ProductRunnerErrorKind::InvalidPrecondition);
        assert!(!error.detail().is_empty());
    }
}
