//! Versioned workspace admission preserves older originals and never substitutes fresh state.

use super::{Operation, Project, Result, Session, Workspace, problem};
use crate::files::attachments::Attachment;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

const VERSION: u16 = 2;

pub(super) struct Decoded {
    pub(super) workspace: Workspace,
    pub(super) operations: BTreeMap<String, Operation>,
    pub(super) migration: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentDocument {
    schema_version: u16,
    workspace: Workspace,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionOneDocument {
    schema_version: u16,
    workspace: LegacyWorkspace,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyWorkspace {
    #[serde(default)]
    identity: String,
    projects: Vec<Project>,
    sessions: Vec<Session>,
    #[serde(default)]
    operations: BTreeMap<String, Operation>,
    attachments: BTreeMap<String, Attachment>,
}

#[derive(Serialize)]
struct Publication<'a> {
    schema_version: u16,
    workspace: &'a Workspace,
}

pub(super) fn decode(path: &Path, bytes: &[u8]) -> Result<Decoded> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        problem(format!(
            "Workspace state at {} is unreadable and was preserved: {error}",
            path.display()
        ))
    })?;
    let version = value.get("schema_version").and_then(serde_json::Value::as_u64);
    let (mut workspace, operations, migration) = match version {
        Some(version) if version == u64::from(VERSION) => {
            let document: CurrentDocument = serde_json::from_value(value).map_err(|error| {
                problem(format!(
                    "Workspace state at {} cannot be admitted and was preserved: {error}",
                    path.display()
                ))
            })?;
            if document.schema_version != VERSION {
                return Err(problem("The admitted workspace schema changed during decoding"));
            }
            (document.workspace, BTreeMap::new(), false)
        }
        Some(1) => {
            let document: VersionOneDocument = serde_json::from_value(value).map_err(|error| {
                problem(format!(
                    "Workspace state at {} cannot be migrated and was preserved: {error}",
                    path.display()
                ))
            })?;
            if document.schema_version != 1 {
                return Err(problem("The legacy workspace schema changed during decoding"));
            }
            let legacy = document.workspace;
            (
                Workspace {
                    identity: legacy.identity,
                    projects: legacy.projects,
                    sessions: legacy.sessions,
                    attachments: legacy.attachments,
                },
                legacy.operations,
                true,
            )
        }
        None => {
            let legacy: LegacyWorkspace = serde_json::from_value(value).map_err(|error| {
                problem(format!(
                    "Legacy workspace state at {} cannot be migrated; its original identities were preserved: {error}",
                    path.display()
                ))
            })?;
            (
                Workspace {
                    identity: legacy.identity,
                    projects: legacy.projects,
                    sessions: legacy.sessions,
                    attachments: legacy.attachments,
                },
                legacy.operations,
                true,
            )
        }
        Some(version) => {
            return Err(problem(format!(
                "Workspace state at {} uses unsupported schema {version}; its original identities were preserved",
                path.display()
            )));
        }
    };
    if workspace.identity.is_empty() {
        workspace.identity = migrated_identity(bytes);
    }
    validate_lineage(&workspace)?;
    Ok(Decoded { workspace, operations, migration })
}

pub(super) fn encode(workspace: &Workspace) -> Result<Vec<u8>> {
    validate_lineage(workspace)?;
    Ok(serde_json::to_vec_pretty(&Publication {
        schema_version: VERSION,
        workspace,
    })?)
}

pub(super) fn preserve_legacy(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| problem("Workspace state has no directory"))?;
    let digest = crate::files::revision(bytes);
    let original = parent.join("workspace-migrations").join(format!("{digest}.legacy.json"));
    match std::fs::read(&original) {
        Ok(retained) if retained == bytes => Ok(()),
        Ok(_) => Err(problem("The retained legacy workspace identity owns different bytes")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => super::save(&original, bytes),
        Err(error) => Err(error.into()),
    }
}

fn migrated_identity(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"peritus/web/migrated-workspace/v1\0");
    hash.update(bytes);
    super::hex(&hash.finalize()[..16])
}

fn validate_lineage(workspace: &Workspace) -> Result<()> {
    let projects: BTreeMap<_, _> =
        workspace.projects.iter().map(|project| (project.id.as_str(), project)).collect();
    let sessions: BTreeMap<_, _> =
        workspace.sessions.iter().map(|session| (session.id.as_str(), session)).collect();
    if projects.len() != workspace.projects.len() || sessions.len() != workspace.sessions.len() {
        return Err(problem(
            "Workspace state has duplicate project or session identities; the original state was preserved",
        ));
    }
    let mut completed = BTreeMap::new();
    for session in &workspace.sessions {
        if let Some(owner) = &session.native { owner.validate_shape()?; }
        if !projects.contains_key(session.project.as_str()) {
            return Err(problem(
                "A retained session has no owning project; the original state was preserved",
            ));
        }
        let mut path = std::collections::BTreeSet::new();
        let mut order = Vec::new();
        let mut current = Some(session.id.as_str());
        let mut inherited = None;
        while let Some(id) = current {
            if let Some(owner) = completed.get(id) {
                inherited = *owner;
                break;
            }
            if !path.insert(id) {
                return Err(problem(
                    "Retained session ancestry contains a cycle; the original state was preserved",
                ));
            }
            order.push(id);
            current = sessions
                .get(id)
                .ok_or_else(|| problem("Retained session ancestry references an unknown parent"))?
                .parent
                .as_deref();
        }
        for id in order.into_iter().rev() {
            let owner = sessions[id].native.as_ref();
            if owner.zip(inherited).is_some_and(|(owner, inherited)| owner != inherited) {
                return Err(problem("Nested sessions belong to different native owners; the original state was preserved"));
            }
            inherited = owner.or(inherited);
            completed.insert(id, inherited);
        }
    }
    Ok(())
}
