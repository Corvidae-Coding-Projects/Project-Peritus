use super::workflow_files::ACTION_DIRECTORY;
use crate::error::Diagnostic;
use std::path::{Component, Path};

const WORKFLOW_DIRECTORY: &str = ".github/workflows";

#[derive(Clone, Copy)]
pub(super) enum LocalUseKind {
    Action,
    Workflow,
}

pub(super) fn validate_local_reference(
    root: &Path,
    action: &str,
    path: &Path,
    location: &str,
    kind: LocalUseKind,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let local = Path::new(action.trim_start_matches("./"));
    let lexical = !action.contains(['\\', '@', '$', '{', '}'])
        && local.components().all(|component| matches!(component, Component::Normal(_)));
    let valid_target = match kind {
        LocalUseKind::Action => local_action_is_composite(root, local),
        LocalUseKind::Workflow => local_workflow_is_confined(root, local),
    };
    if !lexical || !valid_target {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}` uses local reference `{action}` outside checked-in policy"),
            "use a confined composite .github/actions action or checked reusable workflow; executable Node and Docker action payloads are not accepted",
        ));
    }
}

fn local_action_is_composite(root: &Path, local: &Path) -> bool {
    if !local.starts_with(ACTION_DIRECTORY) || has_symlink_component(root, local) {
        return false;
    }
    let target = root.join(local);
    let manifest = [target.join("action.yml"), target.join("action.yaml")]
        .into_iter()
        .find(|path| path.is_file());
    let Some(manifest) = manifest else { return false };
    if !canonical_target_is_confined(root, ACTION_DIRECTORY, &target)
        || !canonical_target_is_confined(root, ACTION_DIRECTORY, &manifest)
    {
        return false;
    }
    std::fs::read_to_string(manifest).ok().is_some_and(|contents| composite_manifest(&contents))
}

fn composite_manifest(contents: &str) -> bool {
    yaml_rust2::YamlLoader::load_from_str(contents)
        .ok()
        .and_then(|documents| documents.into_iter().next())
        .and_then(|document| document.as_hash().cloned())
        .and_then(|mapping| mapping.get(&yaml_rust2::Yaml::String("runs".into())).cloned())
        .and_then(|runs| runs.as_hash().cloned())
        .and_then(|runs| runs.get(&yaml_rust2::Yaml::String("using".into())).cloned())
        .and_then(|using| using.as_str().map(ToOwned::to_owned))
        .is_some_and(|using| using == "composite")
}

fn local_workflow_is_confined(root: &Path, local: &Path) -> bool {
    local.starts_with(WORKFLOW_DIRECTORY)
        && local.extension().is_some_and(|extension| extension == "yml" || extension == "yaml")
        && root.join(local).is_file()
        && canonical_target_is_confined(root, WORKFLOW_DIRECTORY, &root.join(local))
}

fn canonical_target_is_confined(root: &Path, directory: &str, target: &Path) -> bool {
    root.join(directory)
        .canonicalize()
        .ok()
        .zip(target.canonicalize().ok())
        .is_some_and(|(directory, target)| target.starts_with(directory))
}

fn has_symlink_component(root: &Path, local: &Path) -> bool {
    let mut current = root.to_path_buf();
    local.components().any(|component| {
        current.push(component);
        current.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink())
    })
}
