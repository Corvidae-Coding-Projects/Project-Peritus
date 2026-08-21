use super::is_ignored;
use super::trust_discovery::{
    first_symlink_component, normalize, symlink_diagnostic, validate_controlled_input,
};
use crate::error::Diagnostic;
use crate::model::ArchitecturePolicy;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn validate(
    root: &Path,
    declared: &Path,
    policy: &ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PathBuf> {
    let declared_target =
        if declared.is_absolute() { declared.to_path_buf() } else { root.join(declared) };
    let target = normalize(&declared_target);
    let Ok(relative) = target.strip_prefix(root) else {
        diagnostics.push(Diagnostic::at(
            declared,
            "Cargo target compilation source resolves outside the repository",
            "keep every workspace target source in reviewed repository-owned source",
        ));
        return None;
    };
    if is_ignored(relative, &policy.ignored_directories) {
        diagnostics.push(Diagnostic::at(
            relative,
            "Cargo target compilation source is under an ignored repository prefix",
            "move the target root into reviewed source; ignored trees cannot provide compiled code",
        ));
        return None;
    }
    if let Some(component) = first_symlink_component(root, relative) {
        diagnostics.push(Diagnostic::at(
            relative,
            format!(
                "Cargo target compilation source crosses symbolic link component `{}`",
                component.display()
            ),
            "replace every target-path component with repository-owned directories and files",
        ));
        return None;
    }
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            diagnostics.push(Diagnostic::at(
                relative,
                "Cargo target compilation source does not exist",
                "check in the target source declared by Cargo metadata",
            ));
            return None;
        }
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("Cargo target compilation source cannot be inspected: {error}"),
                "make the reviewed target source readable by the trust check",
            ));
            return None;
        }
    };
    if metadata.file_type().is_symlink() {
        diagnostics.push(symlink_diagnostic(relative));
        return None;
    }
    if !metadata.is_file() {
        diagnostics.push(Diagnostic::at(
            relative,
            "Cargo target compilation source is not a regular file",
            "declare one checked-in regular source file as the target root",
        ));
        return None;
    }
    validate_controlled_input(relative, policy, diagnostics);
    Some(target)
}
