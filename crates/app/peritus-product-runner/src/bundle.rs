//! Bounded deterministic repository context construction.

use std::{fmt::Write as _, fs, path::Path, process::Command};

use crate::workspace_filter;
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline, file_metadata,
};

const MAX_FILE_BYTES: usize = 192 * 1024;

pub fn diff(root: &Path, baseline: &CandidateBaseline) -> Result<String, ProductRunnerError> {
    if let Some(scope) = baseline.scope() {
        return scope.diff(root);
    }
    let changed_paths = baseline.changed_paths(root)?;
    if let Some(managed) = baseline.managed() {
        let mut text = metadata_manifest(root, &changed_paths)?;
        text.push_str(&String::from_utf8_lossy(&managed.patch(root)?));
        return Ok(limit_text(&text, 1024 * 1024));
    }
    let output = Command::new("git")
        .args(["-C", root_text(root)?, "diff", "--no-ext-diff"])
        .arg(baseline.head())
        .args(["--", "."])
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
    append_nested_repository_diffs(root, &changed_paths, &mut text)?;
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
        let display = relative.replace('\\', "/");
        append_new_file(diff, &display, &metadata, &content);
    }
    Ok(())
}

fn append_nested_repository_diffs(
    root: &Path,
    changed_paths: &[std::path::PathBuf],
    diff: &mut String,
) -> Result<(), ProductRunnerError> {
    for relative in changed_paths {
        let nested = root.join(relative);
        if !nested.is_dir() || !nested.join(".git").exists() {
            continue;
        }
        let head = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&nested)
            .output()
            .map_err(|error| repository("inspect nested candidate repository", &error))?;
        if !head.status.success() {
            continue;
        }
        let display = relative.to_string_lossy().replace('\\', "/");
        let display = display.trim_end_matches('/');
        let source_prefix = format!("--src-prefix=a/{display}/");
        let destination_prefix = format!("--dst-prefix=b/{display}/");
        let output = Command::new("git")
            .args(["diff", "--no-ext-diff", &source_prefix, &destination_prefix, "HEAD", "--", "."])
            .current_dir(&nested)
            .output()
            .map_err(|error| repository("read nested workspace diff", &error))?;
        if !output.status.success() {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "read nested workspace diff",
                "git could not compare a nested candidate repository with its HEAD",
            ));
        }
        diff.push_str(&String::from_utf8_lossy(&output.stdout));
        append_nested_untracked_files(&nested, relative, diff)?;
    }
    Ok(())
}

fn append_nested_untracked_files(
    nested: &Path,
    prefix: &Path,
    diff: &mut String,
) -> Result<(), ProductRunnerError> {
    let output = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .current_dir(nested)
        .output()
        .map_err(|error| repository("list nested untracked files", &error))?;
    if !output.status.success() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "list nested untracked files",
            "git could not list nested untracked workspace files",
        ));
    }
    for encoded in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        if diff.len() >= 1024 * 1024 {
            break;
        }
        let Ok(relative) = std::str::from_utf8(encoded) else {
            continue;
        };
        let display_path = prefix.join(relative);
        if workspace_filter::generated(&display_path) {
            continue;
        }
        let absolute = nested.join(relative);
        let Ok(metadata) = fs::symlink_metadata(&absolute) else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
            continue;
        }
        let Ok(content) = fs::read_to_string(&absolute) else {
            continue;
        };
        append_new_file(
            diff,
            &display_path.to_string_lossy().replace('\\', "/"),
            &metadata,
            &content,
        );
    }
    Ok(())
}

fn append_new_file(diff: &mut String, display: &str, metadata: &fs::Metadata, content: &str) {
    if content.contains('\0') {
        let _ = writeln!(
            diff,
            "\nBinary file b/{display}: bytes={}, permissions={}; NUL-bearing preview omitted",
            metadata.len(),
            file_metadata::permissions(metadata),
        );
        return;
    }
    let line_count = content.lines().count().max(1);
    let git_mode = file_metadata::git_file_mode(metadata);
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
