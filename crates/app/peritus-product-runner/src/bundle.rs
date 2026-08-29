//! Bounded deterministic repository context construction.

use std::{fmt::Write as _, fs, path::Path, process::Command};

use crate::workspace_filter;
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline, file_metadata,
};

const MAX_FILE_BYTES: usize = 192 * 1024;

pub fn diff(root: &Path) -> Result<String, ProductRunnerError> {
    let baseline = CandidateBaseline::capture(root)?;
    let changed_paths = baseline.changed_paths(root)?;
    let output = Command::new("git")
        .args(["-C", root_text(root)?, "diff", "--no-ext-diff", "--", "."])
        .output()
        .map_err(|error| repository("read workspace diff", &error))?;
    if !output.status.success() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "read workspace diff",
            "git diff failed",
        ));
    }
    let mut text = metadata_manifest(root, &changed_paths)?;
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    append_untracked_files(root, &mut text)?;
    Ok(limit_text(&text, 1024 * 1024))
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
                let _ = writeln!(
                    manifest,
                    "{}: kind={kind}, bytes={}, permissions={}",
                    relative.display(),
                    metadata.len(),
                    file_metadata::permissions(&metadata),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let _ = writeln!(manifest, "{}: missing", relative.display());
            }
            Err(error) => return Err(repository("read workspace metadata", &error)),
        }
    }
    manifest.push('\n');
    Ok(manifest)
}

fn append_untracked_files(root: &Path, diff: &mut String) -> Result<(), ProductRunnerError> {
    let output = Command::new("git")
        .args(["-C", root_text(root)?, "ls-files", "--others", "--exclude-standard", "-z"])
        .output()
        .map_err(|error| repository("list untracked files", &error))?;
    if !output.status.success() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "list untracked files",
            "git could not list untracked workspace files",
        ));
    }
    for encoded in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        if diff.len() >= 1024 * 1024 {
            break;
        }
        let Ok(relative) = std::str::from_utf8(encoded) else {
            continue;
        };
        if workspace_filter::generated(Path::new(relative)) {
            continue;
        }
        let absolute = root.join(relative);
        let Ok(metadata) = fs::symlink_metadata(&absolute) else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
            continue;
        }
        let Ok(content) = fs::read_to_string(&absolute) else {
            continue;
        };
        let line_count = content.lines().count().max(1);
        let display = relative.replace('\\', "/");
        let git_mode = file_metadata::git_file_mode(&metadata);
        let _ = write!(
            diff,
            "\ndiff --git a/{display} b/{display}\nnew file mode {git_mode}\n--- /dev/null\n+++ b/{display}\n@@ -0,0 +1,{line_count} @@\n"
        );
        for line in content.lines() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        if !content.ends_with('\n') {
            diff.push_str("\\ No newline at end of file\n");
        }
    }
    Ok(())
}

fn root_text(root: &Path) -> Result<&str, ProductRunnerError> {
    root.to_str().ok_or_else(|| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "open managed workspace",
            "workspace path is not valid UTF-8",
        )
    })
}

fn repository(operation: &'static str, error: &std::io::Error) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, operation, error.to_string())
}

pub fn limit_text(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    let boundary = value.floor_char_boundary(maximum);
    format!("{}\n[output truncated]", &value[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_includes_untracked_text_files() {
        let temporary = tempfile::tempdir().expect("temporary repository");
        initialize_repository(temporary.path());
        let content = "pub fn ready() -> bool { true }\n";
        fs::write(temporary.path().join("new.rs"), content).expect("write untracked source");

        let actual = diff(temporary.path()).expect("collect diff");

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
        let key = temporary.path().join("server.key");
        fs::write(&key, "private\n").expect("write key");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).expect("set permissions");

        let actual = diff(temporary.path()).expect("collect diff");

        assert!(actual.contains("server.key: kind=file, bytes=8, permissions=0600"));
        assert!(actual.contains("new file mode 100644"));
    }

    fn initialize_repository(root: &Path) {
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(root)
            .status()
            .expect("run git init");
        assert!(init.success());
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
                "fixture",
            ])
            .current_dir(root)
            .status()
            .expect("create fixture commit");
        assert!(commit.success());
    }
}
