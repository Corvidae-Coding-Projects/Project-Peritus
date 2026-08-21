use super::reference::SourceReference;
use super::{controlled_kind, controlled_kind_covers, is_ignored};
use crate::error::{Diagnostic, XtaskError};
use crate::model::{ArchitecturePolicy, ControlledSourceKind};
use std::collections::{BTreeSet, VecDeque};
use std::fs::{self, DirEntry};
use std::path::{Component, Path, PathBuf};

pub(crate) struct Discovery {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub(super) fn discover(
    root: &Path,
    policy: &ArchitecturePolicy,
    target_roots: &[PathBuf],
) -> Result<Discovery, XtaskError> {
    let mut files = BTreeSet::new();
    let mut diagnostics = Vec::new();
    visit_repository(root, root, policy, &mut files, &mut diagnostics)?;
    for target_root in target_roots {
        if let Some(target) =
            super::target_root::validate(root, target_root, policy, &mut diagnostics)
        {
            files.insert(target);
        }
    }

    let mut pending: VecDeque<PathBuf> = files.iter().cloned().collect();
    let mut inspected = BTreeSet::new();
    while let Some(file) = pending.pop_front() {
        if !inspected.insert(file.clone()) {
            continue;
        }
        let relative = file.strip_prefix(root).unwrap_or(&file);
        let bytes = fs::read(&file).map_err(|error| XtaskError::io("read", &file, error))?;
        let Ok(contents) = String::from_utf8(bytes) else {
            diagnostics.push(Diagnostic::at(
                relative,
                "compilation source is not valid UTF-8 and cannot be trust-scanned",
                "store repository compilation inputs as UTF-8 Rust source",
            ));
            continue;
        };
        let scan = super::reference::scan(&contents);
        for import in scan.include_imports {
            diagnostics.push(Diagnostic::at(
                relative,
                format!(
                    "line {} imports or re-exports `include`; aliased source macros cannot be trust-scanned",
                    import.line
                ),
                "remove the use declaration and invoke the built-in `include!` directly with one repository-relative string literal",
            ));
        }
        for reserved in scan.reserved_includes {
            diagnostics.push(Diagnostic::at(
                relative,
                format!(
                    "line {} uses reserved code identifier `include` outside a direct source invocation",
                    reserved.line
                ),
                "rename the identifier or invoke the built-in `include!` directly with one repository-relative string literal",
            ));
        }
        for definition in scan.macro_rules_definitions {
            diagnostics.push(Diagnostic::at(
                relative,
                format!(
                    "line {} defines `macro_rules!`; local macro expansion can synthesize unaudited compilation sources",
                    definition.line
                ),
                "replace the local macro with explicit Rust items and direct literal #[path] or include! source declarations",
            ));
        }
        for reference in scan.references {
            if let Some(target) =
                validate_reference(root, relative, &reference, policy, &mut diagnostics)
                && files.insert(target.clone())
            {
                pending.push_back(target);
            }
        }
    }

    Ok(Discovery { files: files.into_iter().collect(), diagnostics })
}

fn visit_repository(
    root: &Path,
    directory: &Path,
    policy: &ArchitecturePolicy,
    files: &mut BTreeSet<PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let mut entries: Vec<DirEntry> = fs::read_dir(directory)
        .map_err(|error| XtaskError::io("read directory", directory, error))?
        .collect::<Result<_, _>>()
        .map_err(|error| XtaskError::io("read directory entry in", directory, error))?;
    entries.sort_by_key(DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if is_ignored(relative, &policy.ignored_directories) {
            continue;
        }
        let file_type =
            entry.file_type().map_err(|error| XtaskError::io("inspect", &path, error))?;
        if file_type.is_symlink() {
            inspect_repository_symlink(&path, relative, diagnostics);
        } else if file_type.is_dir() {
            visit_repository(root, &path, policy, files, diagnostics)?;
        } else if is_rust_extension(&path) {
            validate_controlled_input(relative, policy, diagnostics);
            files.insert(path);
        }
    }
    Ok(())
}

fn inspect_repository_symlink(path: &Path, relative: &Path, diagnostics: &mut Vec<Diagnostic>) {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => diagnostics.push(Diagnostic::at(
            relative,
            "unignored repository directory is a symbolic link",
            "replace it with a repository-owned directory or anchor its top-level prefix in ignored_directories",
        )),
        Ok(metadata) if metadata.is_file() && is_rust_extension(path) => {
            diagnostics.push(symlink_diagnostic(relative));
        }
        Err(_) => diagnostics.push(Diagnostic::at(
            relative,
            "unignored repository symbolic link is dangling or inaccessible",
            "remove the link or replace it with a reviewed repository-owned file or directory",
        )),
        Ok(_) => {}
    }
}

pub(super) fn validate_reference(
    root: &Path,
    source: &Path,
    reference: &SourceReference,
    policy: &ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PathBuf> {
    let Some(declared) = &reference.path else {
        diagnostics.push(Diagnostic::at(
            source,
            format!(
                "line {} uses a dynamic {} compilation source",
                reference.line,
                reference.kind.label()
            ),
            "use one repository-relative string literal; generated and OUT_DIR inputs are not auditable",
        ));
        return None;
    };
    if declared.is_absolute() {
        diagnostics.push(outside_diagnostic(source, reference));
        return None;
    }
    let base = root.join(source).parent().unwrap_or(root).to_path_buf();
    let target = normalize(&base.join(declared));
    let Ok(relative) = target.strip_prefix(root) else {
        diagnostics.push(outside_diagnostic(source, reference));
        return None;
    };
    if is_ignored(relative, &policy.ignored_directories) {
        diagnostics.push(Diagnostic::at(
            source,
            format!(
                "line {} {} resolves into ignored repository prefix `{}`",
                reference.line,
                reference.kind.label(),
                relative.display()
            ),
            "move the compilation input into reviewed source; ignored trees cannot provide compiled code",
        ));
        return None;
    }
    if let Some(component) = first_symlink_component(root, relative) {
        diagnostics.push(Diagnostic::at(
            relative,
            format!(
                "{} compilation source crosses symbolic link component `{}`",
                reference.kind.label(),
                component.display()
            ),
            "replace every source-path component with repository-owned directories and files",
        ));
        return None;
    }
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            diagnostics.push(Diagnostic::at(
                source,
                format!(
                    "line {} {} compilation source `{}` does not exist",
                    reference.line,
                    reference.kind.label(),
                    relative.display()
                ),
                "check in the referenced source at the static repository-relative path",
            ));
            return None;
        }
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("compilation source cannot be inspected: {error}"),
                "make the reviewed repository source readable by the trust check",
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
            format!("{} compilation source is not a regular file", reference.kind.label()),
            "reference one checked-in regular source file",
        ));
        return None;
    }
    validate_controlled_input(relative, policy, diagnostics);
    Some(target)
}

pub(super) fn first_symlink_component(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return current.strip_prefix(root).ok().map(Path::to_path_buf);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => return Some(current.strip_prefix(root).unwrap_or(&current).to_path_buf()),
        }
    }
    None
}

pub(super) fn validate_controlled_input(
    relative: &Path,
    policy: &ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(kind) = controlled_kind(relative) else { return };
    if controlled_owner(relative, kind, policy).is_none() {
        diagnostics.push(Diagnostic::at(
            relative,
            "generated compilation source has no narrow reviewed ownership root",
            "add a matching controlled_source_roots entry or check in handwritten source outside generated paths",
        ));
    }
}

fn controlled_owner<'a>(
    relative: &Path,
    kind: ControlledSourceKind,
    policy: &'a ArchitecturePolicy,
) -> Option<&'a Path> {
    policy
        .controlled_source_roots
        .iter()
        .filter(|controlled| is_safe_prefix(&controlled.path))
        .filter(|controlled| {
            controlled_kind(&controlled.path)
                .is_some_and(|declared| controlled_kind_covers(controlled.kind, declared))
        })
        .filter(|controlled| {
            !controlled.owner.trim().is_empty() && controlled.rationale.trim().len() >= 20
        })
        .filter(|controlled| relative.starts_with(&controlled.path))
        .filter(|controlled| controlled_kind_covers(controlled.kind, kind))
        .max_by_key(|controlled| controlled.path.components().count())
        .map(|controlled| controlled.path.as_path())
}

fn is_safe_prefix(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| matches!(component, Component::Normal(_)))
}

fn outside_diagnostic(source: &Path, reference: &SourceReference) -> Diagnostic {
    Diagnostic::at(
        source,
        format!(
            "line {} {} compilation source resolves outside the repository",
            reference.line,
            reference.kind.label()
        ),
        "use a static repository-relative source path with no parent traversal beyond the workspace",
    )
}

pub(super) fn symlink_diagnostic(relative: &Path) -> Diagnostic {
    Diagnostic::at(
        relative,
        "compilation source path is a symbolic link and cannot be trust-scanned",
        "replace it with reviewed repository-owned source so the trust check cannot be redirected",
    )
}

fn is_rust_extension(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "rs")
}

pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    normalized
}
