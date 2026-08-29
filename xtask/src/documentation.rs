//! Maintained Markdown inventory, structure, and local-link validation.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Diagnostic, ErrorCode, XtaskError};

const ROOT_FILES: &[&str] = &["README.md", "CHANGELOG.md"];
const ROOTS: &[&str] =
    &["benchmarks", "crates", "docs", "packaging", "release", "security", "verification", "xtask"];

pub(crate) fn check(root: &Path) -> Result<usize, XtaskError> {
    let mut files = Vec::new();
    for relative in ROOT_FILES {
        let path = root.join(relative);
        if path.is_file() {
            files.push(path);
        }
    }
    for relative in ROOTS {
        collect(&root.join(relative), &mut files)?;
    }
    files.sort();
    files.dedup();
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| XtaskError::io("canonicalize documentation root at", root, error))?;
    let mut diagnostics = Vec::new();
    for path in &files {
        validate_file(&canonical_root, path, &mut diagnostics)?;
    }
    if diagnostics.is_empty() {
        Ok(files.len())
    } else {
        Err(XtaskError::violations(ErrorCode::Documentation, "docs-check", diagnostics))
    }
}

fn collect(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), XtaskError> {
    if !directory.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(directory)
        .map_err(|error| XtaskError::io("read documentation directory at", directory, error))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| XtaskError::io("read documentation entry under", directory, error))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| XtaskError::io("inspect documentation entry at", &path, error))?;
        if kind.is_dir() {
            collect(&path, files)?;
        } else if kind.is_file() && path.extension().is_some_and(|value| value == "md") {
            files.push(path);
        }
    }
    Ok(())
}

fn validate_file(
    root: &Path,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let bytes = fs::read(path)
        .map_err(|error| XtaskError::io("read maintained documentation from", path, error))?;
    let relative = path.strip_prefix(root).unwrap_or(path);
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        XtaskError::violations(
            ErrorCode::Documentation,
            "docs-check",
            vec![Diagnostic::at(relative, "documentation is not UTF-8", "save the file as UTF-8")],
        )
    })?;
    validate_structure(relative, text, diagnostics);
    validate_links(root, path, relative, text, diagnostics);
    if is_crate_readme(relative) && !text.contains("\n## Focused checks\n") {
        diagnostics.push(Diagnostic::at(
            relative,
            "crate README does not explain its focused check",
            "add a Focused checks section with a locked package-specific command",
        ));
    }
    Ok(())
}

fn validate_structure(path: &Path, text: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !text.ends_with('\n') {
        diagnostics.push(Diagnostic::at(
            path,
            "documentation has no final newline",
            "end the file with one newline",
        ));
    }
    if text.contains('\r') {
        diagnostics.push(Diagnostic::at(
            path,
            "documentation contains CR line endings",
            "use repository LF line endings",
        ));
    }
    if !text.lines().find(|line| !line.trim().is_empty()).is_some_and(|line| line.starts_with("# "))
    {
        diagnostics.push(Diagnostic::at(
            path,
            "documentation has no leading level-one title",
            "start with one clear `# Title` heading",
        ));
    }
    let mut fenced = false;
    let mut previous_heading = 0_usize;
    for (index, line) in text.lines().enumerate() {
        if line.ends_with(' ') || line.ends_with('\t') {
            diagnostics.push(Diagnostic::at(
                path,
                format!("line {} has trailing whitespace", index + 1),
                "remove the trailing spaces or tab",
            ));
        }
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            let level = line.bytes().take_while(|byte| *byte == b'#').count();
            if level > 0 && line.as_bytes().get(level) == Some(&b' ') {
                if previous_heading > 0 && level > previous_heading + 1 {
                    diagnostics.push(Diagnostic::at(
                        path,
                        format!("line {} skips a Markdown heading level", index + 1),
                        "use consecutive heading levels",
                    ));
                }
                previous_heading = level;
            }
        }
    }
    if fenced {
        diagnostics.push(Diagnostic::at(
            path,
            "documentation has an unclosed code fence",
            "close the final triple-backtick fence",
        ));
    }
}

fn validate_links(
    root: &Path,
    source: &Path,
    relative: &Path,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut fenced = false;
    for (line_index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut remainder = line;
        while let Some(start) = remainder.find("](") {
            let body = &remainder[start + 2..];
            let Some(end) = body.find(')') else { break };
            let target = link_target(&body[..end]);
            if is_local_link(target) && !local_link_exists(root, source, target) {
                diagnostics.push(Diagnostic::at(
                    relative,
                    format!("line {} links to missing local target `{target}`", line_index + 1),
                    "correct the relative path or add the referenced maintained file",
                ));
            }
            remainder = &body[end + 1..];
        }
    }
}

fn link_target(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed.strip_prefix('<').map_or_else(
        || trimmed.split_ascii_whitespace().next().unwrap_or(""),
        |angle| angle.split_once('>').map_or(angle, |(target, _)| target),
    )
}

fn is_local_link(target: &str) -> bool {
    !target.is_empty()
        && !target.starts_with('#')
        && !target.contains("://")
        && !target.starts_with("mailto:")
        && !target.starts_with("app:")
}

fn local_link_exists(root: &Path, source: &Path, target: &str) -> bool {
    let path = target.split_once('#').map_or(target, |(path, _)| path);
    if path.is_empty() {
        return true;
    }
    let candidate = path
        .strip_prefix('/')
        .map_or_else(|| source.parent().unwrap_or(root).join(path), |relative| root.join(relative));
    fs::canonicalize(candidate).is_ok_and(|resolved| resolved.starts_with(root))
}

fn is_crate_readme(path: &Path) -> bool {
    path.starts_with("crates") && path.file_name().is_some_and(|name| name == "README.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_reports_missing_title_and_unclosed_fence() {
        let mut diagnostics = Vec::new();
        validate_structure(
            Path::new("crates/example/README.md"),
            "text\n```sh\n",
            &mut diagnostics,
        );
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn external_and_anchor_links_do_not_require_local_files() {
        for target in ["#section", "https://example.com", "mailto:user@example.com"] {
            assert!(!is_local_link(target));
        }
        assert!(is_local_link("../README.md#section"));
    }
}
