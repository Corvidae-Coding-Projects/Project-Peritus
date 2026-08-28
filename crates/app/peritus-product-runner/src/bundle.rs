//! Bounded deterministic repository context construction.

use std::{
    cmp::Reverse,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_BUNDLE_BYTES: usize = 768 * 1024;
const MAX_FILE_BYTES: usize = 192 * 1024;

pub struct RepositoryBundle {
    pub prompt: String,
}

pub fn build(root: &Path, task: &str) -> Result<RepositoryBundle, ProductRunnerError> {
    let output = Command::new("git")
        .args(["-C", root_text(root)?, "ls-files", "-z"])
        .output()
        .map_err(|error| repository("list tracked files", &error))?;
    if !output.status.success() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "list tracked files",
            "git did not recognize the managed workspace",
        ));
    }
    let mut paths = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .filter_map(|value| std::str::from_utf8(value).ok().map(PathBuf::from))
        .collect::<Vec<_>>();
    let keywords = task
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.len() >= 3)
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| {
        let text = path.to_string_lossy().to_ascii_lowercase();
        let score = keywords.iter().filter(|word| text.contains(word.as_str())).count();
        (Reverse(score), path.clone())
    });

    let mut prompt = String::from("<workspace-files>\n");
    for relative in &paths {
        if prompt.len() >= MAX_BUNDLE_BYTES {
            break;
        }
        let absolute = root.join(relative);
        let Ok(metadata) = fs::symlink_metadata(&absolute) else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
            continue;
        }
        let Ok(bytes) = fs::read(&absolute) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let header = format!("\n<file path=\"{}\">\n", xml_escape(&relative.to_string_lossy()));
        let footer = "\n</file>\n";
        let remaining = MAX_BUNDLE_BYTES.saturating_sub(prompt.len() + header.len() + footer.len());
        if remaining == 0 {
            break;
        }
        prompt.push_str(&header);
        if text.len() <= remaining {
            prompt.push_str(&text);
        } else {
            let boundary = text.floor_char_boundary(remaining);
            prompt.push_str(&text[..boundary]);
            prompt.push_str("\n[truncated]\n");
        }
        prompt.push_str(footer);
    }
    prompt.push_str("</workspace-files>");
    Ok(RepositoryBundle { prompt })
}

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

fn xml_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
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
