//! Bounded candidate previews backed by complete digest-bound evidence.

use std::{fmt::Write as _, fs, path::Path};

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline, file_metadata,
};

#[cfg(test)]
use std::process::Command;

/// Legacy projection used where no exact candidate identity is available. New review boundaries
/// use [`publish`] and retain the complete candidate outside this bounded display value.
pub fn diff(root: &Path, baseline: &CandidateBaseline) -> Result<String, ProductRunnerError> {
    if let Some(scope) = baseline.scope() {
        return scope.diff(root);
    }
    let changed_paths = baseline.changed_paths(root)?;
    let mut text = metadata_manifest(root, &changed_paths)?;
    text.insert_str(0, "Legacy bounded candidate metadata; complete per-file evidence was not published at this call site.\n");
    Ok(limit_text(&text, 256 * 1024))
}

pub fn publish(
    root: &Path,
    baseline: &CandidateBaseline,
    trace: &Path,
    identity: peritus_run_settlement::CandidateIdentity,
    cancellation: crate::candidate::process::Cancellation,
) -> Result<Option<String>, ProductRunnerError> {
    crate::candidate::evidence::publish(root, baseline, trace, identity, cancellation)
        .map(|published| published.map(crate::candidate::evidence::Published::preview))
}

fn metadata_manifest(
    root: &Path,
    changed_paths: &[std::path::PathBuf],
) -> Result<String, ProductRunnerError> {
    let mut manifest = String::from(
        "Peritus current workspace metadata (authoritative; Git modes record only the executable bit):\n",
    );
    for relative in changed_paths {
        let absolute = root.join(relative);
        match fs::symlink_metadata(&absolute) {
            Ok(metadata) => {
                let kind = if metadata.is_dir() { "directory" } else { "file" };
                let _ = write!(
                    manifest,
                    "{}: kind={kind}, bytes={}, permissions={}",
                    relative.display(),
                    metadata.len(),
                    file_metadata::permissions(&metadata),
                );
                manifest.push('\n');
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let _ = write!(manifest, "{}: missing", relative.display());
                manifest.push('\n');
            }
            Err(error) => return Err(repository("read workspace metadata", &error)),
        }
    }
    manifest.push('\n');
    Ok(manifest)
}

fn repository(operation: &'static str, error: &std::io::Error) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, operation, error.to_string())
}

pub fn limit_text(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    const SUFFIX: &str = "\n[output truncated]";
    if maximum <= SUFFIX.len() {
        return SUFFIX[..SUFFIX.floor_char_boundary(maximum)].to_owned();
    }
    let boundary = value.floor_char_boundary(maximum - SUFFIX.len());
    format!("{}{SUFFIX}", &value[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_includes_untracked_text_files() {
        let temporary = tempfile::tempdir().expect("temporary repository");
        initialize_repository(temporary.path());
        let baseline = CandidateBaseline::capture(temporary.path()).expect("baseline");
        let content = "pub fn ready() -> bool { true }\n";
        fs::write(temporary.path().join("new.rs"), content).expect("write untracked source");

        let actual = diff(temporary.path(), &baseline).expect("collect diff");

        assert!(actual.contains("diff --git a/new.rs b/new.rs"));
        assert!(actual.contains("+pub fn ready() -> bool { true }"));
        assert!(
            actual.contains(&format!("new.rs: kind=file, bytes={}, permissions=", content.len()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn diff_reports_exact_permissions_separately_from_git_mode() {
        use std::os::unix::fs::PermissionsExt as _;

        let temporary = tempfile::tempdir().expect("temporary repository");
        initialize_repository(temporary.path());
        let baseline = CandidateBaseline::capture(temporary.path()).expect("baseline");
        let key = temporary.path().join("server.key");
        fs::write(&key, "private\n").expect("write key");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).expect("set permissions");

        let actual = diff(temporary.path(), &baseline).expect("collect diff");

        assert!(actual.contains("server.key: kind=file, bytes=8, permissions=0600"));
        assert!(actual.contains("new file mode 100644"));
    }

    #[test]
    fn diff_projects_dirty_nested_repository_files_into_outer_context() {
        let temporary = tempfile::tempdir().expect("temporary repository");
        initialize_repository(temporary.path());
        let baseline = CandidateBaseline::capture(temporary.path()).expect("baseline");
        let nested = temporary.path().join("imported");
        fs::create_dir(&nested).expect("nested repository");
        initialize_repository(&nested);
        fs::write(nested.join("tracked.rs"), "pub const VALUE: u8 = 1;\n").expect("tracked source");
        let add = Command::new("git")
            .args(["add", "."])
            .current_dir(&nested)
            .status()
            .expect("stage nested source");
        assert!(add.success());
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=Peritus Test",
                "-c",
                "user.email=peritus@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "nested fixture",
            ])
            .current_dir(&nested)
            .status()
            .expect("commit nested source");
        assert!(commit.success());
        fs::write(nested.join("tracked.rs"), "pub const VALUE: u8 = 2;\n").expect("modify source");
        fs::write(nested.join("new.rs"), "pub const NEW: u8 = 3;\n").expect("new source");

        let actual = diff(temporary.path(), &baseline).expect("collect diff");

        assert!(
            actual.contains("diff --git a/imported/tracked.rs b/imported/tracked.rs"),
            "{actual}"
        );
        assert!(actual.contains("diff --git a/imported/new.rs b/imported/new.rs"), "{actual}");
        assert!(actual.contains("+pub const VALUE: u8 = 2;"));
        assert!(actual.contains("+pub const NEW: u8 = 3;"));
    }

    #[test]
    fn diff_includes_changes_committed_after_the_run_baseline() {
        let temporary = tempfile::tempdir().expect("temporary repository");
        initialize_repository(temporary.path());
        let baseline = CandidateBaseline::capture(temporary.path()).expect("baseline");
        fs::write(temporary.path().join("committed.txt"), "retained\n").expect("source");
        let add = Command::new("git")
            .args(["add", "committed.txt"])
            .current_dir(temporary.path())
            .status()
            .expect("stage source");
        assert!(add.success());
        commit(temporary.path(), "committed candidate");

        let actual = diff(temporary.path(), &baseline).expect("collect committed diff");

        assert!(actual.contains("diff --git a/committed.txt b/committed.txt"), "{actual}");
        assert!(actual.contains("+retained"), "{actual}");
    }

    fn initialize_repository(root: &Path) {
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(root)
            .status()
            .expect("run git init");
        assert!(init.success());
        commit(root, "fixture");
    }

    fn commit(root: &Path, message: &str) {
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=Peritus Test",
                "-c",
                "user.email=peritus@example.invalid",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                message,
            ])
            .current_dir(root)
            .status()
            .expect("create fixture commit");
        assert!(commit.success());
    }
}
