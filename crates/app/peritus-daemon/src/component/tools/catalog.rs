//! Explicit compiled descriptor inventory; no filesystem or plugin discovery occurs here.

use std::collections::BTreeMap;
use std::sync::Arc;

use peritus_tool_protocol::ToolDescriptor;
use peritus_tools_fs::descriptor_catalog as fs_descriptors;
use peritus_tools_git::descriptor_catalog as git_descriptors;
use peritus_tools_quality::{discover_descriptor, run_descriptor};
use peritus_tools_shell::{exec_descriptor, script_descriptor};

use super::{
    FilesystemDispatcherRoute, GitDispatcherRoute, ToolComponentError, ToolComponentErrorKind,
    ToolDispatcherRoute,
};

pub(super) struct ToolDeclaration {
    pub route: ToolDispatcherRoute,
    pub descriptor: Arc<ToolDescriptor>,
}

pub(super) fn production_catalog() -> Result<Vec<ToolDeclaration>, ToolComponentError> {
    let mut declarations = Vec::new();
    let mut filesystem = index(
        "filesystem",
        fs_descriptors().map_err(|error| {
            catalog_failure("construct filesystem descriptor catalog", error.to_string())
        })?,
    )?;
    for route in [
        FilesystemDispatcherRoute::Create,
        FilesystemDispatcherRoute::Discover,
        FilesystemDispatcherRoute::Metadata,
        FilesystemDispatcherRoute::Patch,
        FilesystemDispatcherRoute::Read,
        FilesystemDispatcherRoute::Remove,
        FilesystemDispatcherRoute::Replace,
        FilesystemDispatcherRoute::Search,
        FilesystemDispatcherRoute::Write,
    ] {
        take(&mut declarations, &mut filesystem, ToolDispatcherRoute::Filesystem(route))?;
    }
    reject_catalog_remainder("filesystem", &filesystem)?;

    let mut git = index(
        "Git",
        git_descriptors().map_err(|error| {
            catalog_failure("construct Git descriptor catalog", error.to_string())
        })?,
    )?;
    for route in [
        GitDispatcherRoute::Candidate,
        GitDispatcherRoute::Diff,
        GitDispatcherRoute::History,
        GitDispatcherRoute::Rollback,
        GitDispatcherRoute::Snapshot,
        GitDispatcherRoute::Status,
    ] {
        take(&mut declarations, &mut git, ToolDispatcherRoute::Git(route))?;
    }
    // C1 does not yet publish a merge effect. Its typed unsupported adapter is deliberately not a
    // production route, so configuration cannot expose a handler which never performs the tool.
    let _unsupported_merge = git.remove("git.merge").ok_or_else(|| {
        catalog_failure("construct Git descriptor catalog", "git.merge descriptor is absent")
    })?;
    reject_catalog_remainder("Git", &git)?;

    insert(
        &mut declarations,
        ToolDispatcherRoute::QualityDiscover,
        discover_descriptor().map_err(|error| {
            catalog_failure("construct quality discovery descriptor", error.to_string())
        })?,
    )?;
    insert(
        &mut declarations,
        ToolDispatcherRoute::QualityRun,
        run_descriptor().map_err(|error| {
            catalog_failure("construct quality run descriptor", error.to_string())
        })?,
    )?;
    insert(
        &mut declarations,
        ToolDispatcherRoute::ShellExec,
        exec_descriptor().map_err(|error| {
            catalog_failure("construct shell exec descriptor", error.to_string())
        })?,
    )?;
    insert(
        &mut declarations,
        ToolDispatcherRoute::ShellScript,
        script_descriptor().map_err(|error| {
            catalog_failure("construct shell script descriptor", error.to_string())
        })?,
    )?;

    declarations.sort_by(|left, right| {
        (left.descriptor.name().as_str(), left.descriptor.version())
            .cmp(&(right.descriptor.name().as_str(), right.descriptor.version()))
    });
    if declarations.windows(2).any(|pair| {
        (pair[0].descriptor.name().as_str(), pair[0].descriptor.version())
            >= (pair[1].descriptor.name().as_str(), pair[1].descriptor.version())
    }) {
        return Err(catalog_failure(
            "construct production tool catalog",
            "compiled descriptor identities are not unique and canonical",
        ));
    }
    Ok(declarations)
}

fn index(
    family: &'static str,
    descriptors: Vec<ToolDescriptor>,
) -> Result<BTreeMap<String, ToolDescriptor>, ToolComponentError> {
    let mut indexed = BTreeMap::new();
    for descriptor in descriptors {
        let name = descriptor.name().as_str().to_owned();
        if indexed.insert(name, descriptor).is_some() {
            return Err(catalog_failure(
                "index compiled tool descriptors",
                format!("{family} catalog repeats a capability name"),
            ));
        }
    }
    Ok(indexed)
}

fn take(
    target: &mut Vec<ToolDeclaration>,
    indexed: &mut BTreeMap<String, ToolDescriptor>,
    route: ToolDispatcherRoute,
) -> Result<(), ToolComponentError> {
    let descriptor = indexed.remove(route.name()).ok_or_else(|| {
        catalog_failure(
            "bind production dispatcher route",
            format!("compiled descriptor {} is absent", route.name()),
        )
    })?;
    insert(target, route, descriptor)
}

fn insert(
    target: &mut Vec<ToolDeclaration>,
    route: ToolDispatcherRoute,
    descriptor: ToolDescriptor,
) -> Result<(), ToolComponentError> {
    if descriptor.name().as_str() != route.name() {
        return Err(catalog_failure(
            "bind production dispatcher route",
            "descriptor name differs from its explicit dispatcher route",
        ));
    }
    target.push(ToolDeclaration { route, descriptor: Arc::new(descriptor) });
    Ok(())
}

fn reject_catalog_remainder(
    family: &'static str,
    indexed: &BTreeMap<String, ToolDescriptor>,
) -> Result<(), ToolComponentError> {
    if indexed.is_empty() {
        return Ok(());
    }
    Err(catalog_failure(
        "construct production tool catalog",
        format!("{family} catalog contains an undeclared production route"),
    ))
}

fn catalog_failure(operation: &'static str, detail: impl Into<String>) -> ToolComponentError {
    ToolComponentError::new(ToolComponentErrorKind::Catalog, operation, detail)
}
