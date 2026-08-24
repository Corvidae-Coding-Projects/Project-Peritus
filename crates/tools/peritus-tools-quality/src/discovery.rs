//! Deterministic explicit/Cargo/Just quality catalog discovery.

use std::collections::BTreeMap;

use peritus_patch::WorkspacePath;
use peritus_types::GateId;
use peritus_workspace::{ReadOnlyWorkspace, WorkspaceEntryKind};
use sha2::{Digest, Sha256};

use crate::{
    CheckDefinition, CheckRequirement, CheckSource, EnvironmentProfile, ExpectedSuccess,
    OutputParser, QualityError, QualityErrorKind,
};

mod cargo;
mod just;

const MANIFEST_BOUND: u64 = 1024 * 1024;

/// One catalog entry preserving its complete definition and provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredCheck(CheckDefinition);

impl DiscoveredCheck {
    /// Returns the complete invocable definition.
    #[must_use]
    pub const fn definition(&self) -> &CheckDefinition {
        &self.0
    }
}

/// Canonically ordered unique quality check catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckCatalog {
    checks: Vec<DiscoveredCheck>,
}

impl CheckCatalog {
    /// Builds a deterministic catalog from caller-supplied typed definitions.
    ///
    /// # Errors
    /// Returns a typed failure when stable gate names collide.
    pub fn from_explicit(definitions: Vec<CheckDefinition>) -> Result<Self, QualityError> {
        canonical_catalog(definitions)
    }

    /// Returns canonical entries sorted by stable gate name.
    #[must_use]
    pub fn checks(&self) -> &[DiscoveredCheck] {
        &self.checks
    }

    /// Finds one exact stable gate name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&CheckDefinition> {
        self.checks
            .binary_search_by(|entry| entry.0.gate_name().cmp(name))
            .ok()
            .map(|index| &self.checks[index].0)
    }
}

pub fn inspect(
    workspace: &ReadOnlyWorkspace,
    explicit: Vec<CheckDefinition>,
) -> Result<CheckCatalog, QualityError> {
    let entries = workspace.list_directory(None)?;
    let cargo = if regular_file(&entries, "Cargo.toml") {
        let path = WorkspacePath::new("Cargo.toml").map_err(|error| {
            QualityError::new(QualityErrorKind::InvalidInput, error.to_string())
        })?;
        Some(workspace.read_file(&path, MANIFEST_BOUND)?)
    } else {
        None
    };
    let mut justfiles = Vec::new();
    for name in ["Justfile", "justfile"] {
        if regular_file(&entries, name) {
            let path = WorkspacePath::new(name).map_err(|error| {
                QualityError::new(QualityErrorKind::InvalidInput, error.to_string())
            })?;
            justfiles.push((name.to_owned(), workspace.read_file(&path, MANIFEST_BOUND)?));
        }
    }
    from_surfaces(explicit, cargo.as_deref(), &justfiles)
}

fn from_surfaces(
    explicit: Vec<CheckDefinition>,
    cargo_manifest: Option<&[u8]>,
    justfiles: &[(String, Vec<u8>)],
) -> Result<CheckCatalog, QualityError> {
    let mut definitions = explicit;
    if let Some(manifest) = cargo_manifest {
        cargo::discover(manifest, &mut definitions)?;
    }
    for (name, bytes) in justfiles {
        just::discover(name, bytes, &mut definitions)?;
    }
    canonical_catalog(definitions)
}

fn regular_file(entries: &[peritus_workspace::DirectoryEntry], name: &str) -> bool {
    entries.iter().any(|entry| {
        entry.metadata().path().as_str() == name
            && entry.metadata().kind() == WorkspaceEntryKind::File
    })
}

fn canonical_catalog(definitions: Vec<CheckDefinition>) -> Result<CheckCatalog, QualityError> {
    let mut by_name = BTreeMap::new();
    for definition in definitions {
        let name = definition.gate_name().to_owned();
        if by_name.insert(name, DiscoveredCheck(definition)).is_some() {
            return Err(QualityError::new(
                QualityErrorKind::InvalidInput,
                "quality catalog contains a duplicate gate name",
            ));
        }
    }
    Ok(CheckCatalog { checks: by_name.into_values().collect() })
}

pub fn discovered_definition(
    gate_name: &str,
    source: CheckSource,
    executable: &str,
    arguments: Vec<String>,
) -> Result<CheckDefinition, QualityError> {
    CheckDefinition::new(
        gate_name.to_owned(),
        derived_gate_id(gate_name, executable, &arguments),
        source,
        CheckRequirement::Discovered,
        executable,
        arguments,
        None,
        EnvironmentProfile::new("quality-default")?,
        600_000,
        8 * 1024 * 1024,
        OutputParser::None,
        ExpectedSuccess::ExitCode(0),
    )
}

fn derived_gate_id(name: &str, executable: &str, arguments: &[String]) -> GateId {
    let mut hash = Sha256::new();
    hash.update(b"peritus-c4-discovered-gate-v1");
    hash.update((name.len() as u64).to_le_bytes());
    hash.update(name.as_bytes());
    hash.update((executable.len() as u64).to_le_bytes());
    hash.update(executable.as_bytes());
    for argument in arguments {
        hash.update((argument.len() as u64).to_le_bytes());
        hash.update(argument.as_bytes());
    }
    let digest: [u8; 32] = hash.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    if bytes == [0; 16] {
        bytes[15] = 1;
    }
    GateId::new(bytes).expect("derived nonzero gate identifier")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn real_temporary_project_discovers_cargo_and_just_surfaces() {
        let project = tempfile::tempdir().expect("temporary project");
        let cargo_path = project.path().join("Cargo.toml");
        let just_path = project.path().join("Justfile");
        fs::write(&cargo_path, "[package]\nname='fixture'\nversion='0.1.0'\n").expect("Cargo.toml");
        fs::write(&just_path, "verify:\n  cargo test\n").expect("Justfile");
        let cargo = fs::read(cargo_path).expect("read Cargo.toml");
        let just = fs::read(just_path).expect("read Justfile");
        let catalog = from_surfaces(Vec::new(), Some(&cargo), &[("Justfile".to_owned(), just)])
            .expect("catalog");
        let names: Vec<_> =
            catalog.checks().iter().map(|check| check.definition().gate_name()).collect();
        assert_eq!(
            names,
            ["cargo.check", "cargo.clippy", "cargo.fmt", "cargo.test", "just.verify"]
        );
        assert!(
            catalog
                .checks()
                .iter()
                .all(|check| { check.definition().requirement() == CheckRequirement::Discovered })
        );
    }
}
