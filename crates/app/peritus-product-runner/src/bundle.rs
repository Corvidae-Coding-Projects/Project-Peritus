//! Bounded deterministic repository context construction.

use std::{fmt::Write as _, fs, path::Path, process::Command};

use crate::workspace_filter;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_FILE_BYTES: usize = 192 * 1024;

pub fn diff(root: &Path) -> Result<String, ProductRunnerError> {
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
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    append_untracked_files(root, &mut text)?;
    Ok(limit_text(&text, 1024 * 1024))
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
        let _ = write!(
            diff,
            "\ndiff --git a/{display} b/{display}\nnew file mode 100644\n--- /dev/null\n+++ b/{display}\n@@ -0,0 +1,{line_count} @@\n"
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
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(temporary.path())
            .status()
            .expect("run git init");
        assert!(status.success());
        fs::write(temporary.path().join("new.rs"), "pub fn ready() -> bool { true }\n")
            .expect("write untracked source");

        let actual = diff(temporary.path()).expect("collect diff");

        assert!(actual.contains("diff --git a/new.rs b/new.rs"));
        assert!(actual.contains("+pub fn ready() -> bool { true }"));
    }
}
