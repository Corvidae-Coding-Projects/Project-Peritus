//! Deterministic explicit/Cargo/Just quality catalog discovery.

use std::{cmp::Ordering, collections::BTreeMap, ffi::OsStr};

use peritus_types::{GateId, Sha256Digest};
use peritus_workspace::{
    DirectoryListing, FileReadSelection, ReadOnlyWorkspace, WorkspaceEntryKind,
    WorkspaceMetadata,
};
use sha2::{Digest, Sha256};

use crate::{
    CheckDefinition, CheckRequirement, CheckSource, EnvironmentProfile, ExpectedSuccess,
    OutputParser, QualityError,
};

mod cargo;
mod just;

/// Completeness of one discovery source after bounded inspection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiscoveryCoverage {
    /// The supported source grammar was inspected completely.
    Complete,
    /// Usable checks were retained, but some source syntax or metadata was not resolved.
    Partial,
    /// The source could not be inspected safely.
    Unavailable,
}

impl DiscoveryCoverage {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Stable reason that discovery could not claim complete source coverage.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiscoveryCause {
    /// A source could not be read through the immutable workspace authority.
    SourceUnavailable,
    /// The source was malformed for its declared format.
    InvalidSyntax,
    /// The source used valid or potentially valid syntax outside the bounded grammar.
    UnsupportedSyntax,
    /// Optional executable/component availability was not established by inspected metadata.
    ToolMetadataUnavailable,
    /// Two definitions claimed the same stable gate name and one was reconciled deterministically.
    NameCollision,
}

impl DiscoveryCause {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::SourceUnavailable => "source-unavailable",
            Self::InvalidSyntax => "invalid-syntax",
            Self::UnsupportedSyntax => "unsupported-syntax",
            Self::ToolMetadataUnavailable => "tool-metadata-unavailable",
            Self::NameCollision => "name-collision",
        }
    }
}

/// Bounded evidence explaining incomplete discovery without discarding usable checks.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DiscoveryDiagnostic {
    source: String,
    cause: DiscoveryCause,
    coverage: DiscoveryCoverage,
    detail: String,
}

impl DiscoveryDiagnostic {
    pub(super) fn new(
        source: impl Into<String>,
        cause: DiscoveryCause,
        coverage: DiscoveryCoverage,
        detail: impl Into<String>,
    ) -> Self {
        let mut source = source.into();
        let mut detail = detail.into();
        crate::error::truncate_utf8(&mut source, 512);
        crate::error::truncate_utf8(&mut detail, 4_096);
        Self { source, cause, coverage, detail }
    }

    /// Returns the inspected source label.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the stable diagnostic cause.
    #[must_use]
    pub const fn cause(&self) -> DiscoveryCause {
        self.cause
    }

    /// Returns the established coverage for this source.
    #[must_use]
    pub const fn coverage(&self) -> DiscoveryCoverage {
        self.coverage
    }

    /// Returns bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

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

/// Canonically ordered unique quality check catalog with explicit discovery coverage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckCatalog {
    checks: Vec<DiscoveredCheck>,
    diagnostics: Vec<DiscoveryDiagnostic>,
    digest: Sha256Digest,
}

impl CheckCatalog {
    /// Builds a deterministic catalog from caller-supplied typed definitions.
    ///
    /// Duplicate stable names are reconciled deterministically and retained as diagnostics rather
    /// than making every unrelated explicit definition unavailable.
    ///
    /// # Errors
    /// This compatibility-returning constructor currently has no fallible catalog step.
    pub fn from_explicit(definitions: Vec<CheckDefinition>) -> Result<Self, QualityError> {
        Ok(canonical_catalog(definitions, Vec::new()))
    }

    /// Returns canonical entries sorted by stable gate name.
    #[must_use]
    pub fn checks(&self) -> &[DiscoveredCheck] {
        &self.checks
    }

    /// Returns canonical diagnostics for unavailable, partial, or reconciled sources.
    #[must_use]
    pub fn diagnostics(&self) -> &[DiscoveryDiagnostic] {
        &self.diagnostics
    }

    /// Returns whether every inspected source was covered without an omission or collision.
    #[must_use]
    pub fn coverage_complete(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Returns the immutable digest of every retained definition and discovery diagnostic.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
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
    let mut definitions = explicit;
    let mut diagnostics = Vec::new();
    let entries = match workspace.inspect_directory(None) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(DiscoveryDiagnostic::new(
                "workspace-root",
                DiscoveryCause::SourceUnavailable,
                DiscoveryCoverage::Unavailable,
                error.to_string(),
            ));
            return Ok(canonical_catalog(definitions, diagnostics));
        }
    };

    let cargo_manifest = read_root_source(workspace, &entries, "Cargo.toml", &mut diagnostics);
    let mut tool_metadata = Vec::new();
    for name in ["rust-toolchain.toml", "rust-toolchain"] {
        if let Some(bytes) = read_root_source(workspace, &entries, name, &mut diagnostics) {
            tool_metadata.push((name.to_owned(), bytes));
        }
    }
    if let Some(manifest) = cargo_manifest {
        cargo::discover(&manifest, &tool_metadata, &mut definitions, &mut diagnostics);
    }
    for name in ["Justfile", "justfile"] {
        discover_just_source(
            workspace,
            &entries,
            name,
            &mut definitions,
            &mut diagnostics,
        );
    }
    Ok(canonical_catalog(definitions, diagnostics))
}

fn read_root_source(
    workspace: &ReadOnlyWorkspace,
    entries: &DirectoryListing,
    name: &str,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) -> Option<Vec<u8>> {
    let metadata = root_source_metadata(entries, name, diagnostics)?;
    let mut bytes = Vec::new();
    match workspace.copy_selection(metadata.path(), FileReadSelection::all(), &mut bytes) {
        Ok(_) => Some(bytes),
        Err(error) => {
            diagnostics.push(DiscoveryDiagnostic::new(
                name,
                DiscoveryCause::SourceUnavailable,
                DiscoveryCoverage::Unavailable,
                error.to_string(),
            ));
            None
        }
    }
}

fn discover_just_source(
    workspace: &ReadOnlyWorkspace,
    entries: &DirectoryListing,
    name: &str,
    definitions: &mut Vec<CheckDefinition>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    let Some(metadata) = root_source_metadata(entries, name, diagnostics) else { return };
    let mut discovery = just::StreamingDiscovery::new(name);
    match workspace.copy_selection(metadata.path(), FileReadSelection::all(), &mut discovery) {
        Ok(_) => {
            let (discovered, observed) = discovery.finish();
            definitions.extend(discovered);
            diagnostics.extend(observed);
        }
        Err(error) => diagnostics.push(DiscoveryDiagnostic::new(
            name,
            DiscoveryCause::SourceUnavailable,
            DiscoveryCoverage::Unavailable,
            error.to_string(),
        )),
    }
}

fn root_source_metadata<'listing>(
    entries: &'listing DirectoryListing,
    name: &str,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) -> Option<&'listing WorkspaceMetadata> {
    let Some(item) = entries.items().iter().find(|item| {
        item.name()
            .native_name()
            .is_ok_and(|native| native.as_os_str() == OsStr::new(name))
    }) else {
        return None;
    };
    match item.observation() {
        Ok(metadata) if metadata.kind() == WorkspaceEntryKind::File => Some(metadata),
        Ok(_) => {
            diagnostics.push(DiscoveryDiagnostic::new(
                name,
                DiscoveryCause::SourceUnavailable,
                DiscoveryCoverage::Unavailable,
                "root discovery source is not a regular file",
            ));
            return None;
        }
        Err(reason) => {
            diagnostics.push(DiscoveryDiagnostic::new(
                name,
                DiscoveryCause::SourceUnavailable,
                DiscoveryCoverage::Unavailable,
                format!(
                    "root discovery source was excluded by no-follow inspection: {}",
                    reason.as_str()
                ),
            ));
            return None;
        }
    }
}

fn canonical_catalog(
    definitions: Vec<CheckDefinition>,
    mut diagnostics: Vec<DiscoveryDiagnostic>,
) -> CheckCatalog {
    let mut by_name: BTreeMap<String, DiscoveredCheck> = BTreeMap::new();
    for definition in definitions {
        let name = definition.gate_name().to_owned();
        if let Some(existing) = by_name.get(&name) {
            let replace = preferred_definition(&definition, &existing.0) == Ordering::Less;
            let (retained, omitted) = if replace {
                (source_label(definition.source()), source_label(existing.0.source()))
            } else {
                (source_label(existing.0.source()), source_label(definition.source()))
            };
            diagnostics.push(DiscoveryDiagnostic::new(
                format!("gate:{name}"),
                DiscoveryCause::NameCollision,
                DiscoveryCoverage::Partial,
                format!(
                    "stable gate name collision retained {retained} and omitted {omitted}; explicit definitions precede Cargo, which precedes Just, with canonical identity breaking same-source ties"
                ),
            ));
            if replace {
                by_name.insert(name, DiscoveredCheck(definition));
            }
        } else {
            by_name.insert(name, DiscoveredCheck(definition));
        }
    }
    diagnostics.sort_unstable();
    let checks = by_name.into_values().collect::<Vec<_>>();
    let digest = catalog_digest(&checks, &diagnostics);
    CheckCatalog { checks, diagnostics, digest }
}

fn preferred_definition(left: &CheckDefinition, right: &CheckDefinition) -> Ordering {
    source_precedence(left.source())
        .cmp(&source_precedence(right.source()))
        .then_with(|| definition_identity(left).cmp(&definition_identity(right)))
}

const fn source_precedence(source: &CheckSource) -> u8 {
    match source {
        CheckSource::Explicit(_) => 0,
        CheckSource::CargoManifest => 1,
        CheckSource::JustfileRecipe(_) => 2,
    }
}

fn source_label(source: &CheckSource) -> String {
    match source {
        CheckSource::Explicit(label) => format!("explicit:{label}"),
        CheckSource::CargoManifest => "cargo-manifest".to_owned(),
        CheckSource::JustfileRecipe(recipe) => format!("justfile:{recipe}"),
    }
}

fn catalog_digest(
    checks: &[DiscoveredCheck],
    diagnostics: &[DiscoveryDiagnostic],
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus-quality-discovery-catalog-v1\0");
    hash.update((checks.len() as u64).to_be_bytes());
    for check in checks {
        put_hash(&mut hash, &definition_identity(check.definition()));
    }
    hash.update((diagnostics.len() as u64).to_be_bytes());
    for diagnostic in diagnostics {
        put_hash(&mut hash, diagnostic.source.as_bytes());
        hash.update([diagnostic.cause as u8, diagnostic.coverage as u8]);
        put_hash(&mut hash, diagnostic.detail.as_bytes());
    }
    Sha256Digest::new(hash.finalize().into())
}

fn definition_identity(definition: &CheckDefinition) -> Vec<u8> {
    let mut bytes = b"peritus-quality-discovered-definition-v1\0".to_vec();
    put_bytes(&mut bytes, definition.gate_name().as_bytes());
    put_bytes(&mut bytes, definition.gate_id().as_bytes());
    match definition.source() {
        CheckSource::Explicit(label) => {
            bytes.push(1);
            put_bytes(&mut bytes, label.as_bytes());
        }
        CheckSource::CargoManifest => bytes.push(2),
        CheckSource::JustfileRecipe(recipe) => {
            bytes.push(3);
            put_bytes(&mut bytes, recipe.as_bytes());
        }
    }
    bytes.push(match definition.requirement() {
        CheckRequirement::Required => 1,
        CheckRequirement::Optional => 2,
        CheckRequirement::Discovered => 3,
    });
    put_bytes(&mut bytes, definition.executable().as_bytes());
    bytes.extend_from_slice(&(definition.arguments().len() as u64).to_be_bytes());
    for argument in definition.arguments() {
        put_bytes(&mut bytes, argument.as_bytes());
    }
    put_optional(
        &mut bytes,
        definition.working_directory().map(|path| path.as_str().as_bytes()),
    );
    put_bytes(&mut bytes, definition.environment_profile().as_str().as_bytes());
    put_optional_u64(&mut bytes, definition.timeout_millis());
    put_optional_u64(&mut bytes, definition.output_limit());
    match definition.parser() {
        OutputParser::None => bytes.push(0),
        OutputParser::Utf8 { maximum_bytes } => {
            bytes.push(1);
            bytes.extend_from_slice(&maximum_bytes.to_be_bytes());
        }
        OutputParser::Json { maximum_bytes } => {
            bytes.push(2);
            bytes.extend_from_slice(&maximum_bytes.to_be_bytes());
        }
        OutputParser::JsonSuccess { maximum_bytes } => {
            bytes.push(3);
            bytes.extend_from_slice(&maximum_bytes.to_be_bytes());
        }
    }
    match definition.expected_success() {
        ExpectedSuccess::ExitCode(code) => {
            bytes.push(1);
            bytes.extend_from_slice(&code.to_be_bytes());
        }
    }
    bytes
}

fn put_hash(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value);
}

fn put_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

fn put_optional(target: &mut Vec<u8>, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            target.push(1);
            put_bytes(target, value);
        }
        None => target.push(0),
    }
}

fn put_optional_u64(target: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            target.push(1);
            target.extend_from_slice(&value.to_be_bytes());
        }
        None => target.push(0),
    }
}

pub fn discovered_definition(
    gate_name: &str,
    source: CheckSource,
    executable: &str,
    arguments: Vec<String>,
) -> Result<CheckDefinition, QualityError> {
    CheckDefinition::with_optional_limits(
        gate_name.to_owned(),
        derived_gate_id(gate_name, executable, &arguments),
        source,
        CheckRequirement::Discovered,
        executable,
        arguments,
        None,
        EnvironmentProfile::new("quality-default")?,
        None,
        None,
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
        assert_eq!(names, ["cargo.check", "cargo.test", "just.verify"]);
        assert!(
            catalog
                .checks()
                .iter()
                .all(|check| { check.definition().requirement() == CheckRequirement::Discovered })
        );
    }

    fn from_surfaces(
        explicit: Vec<CheckDefinition>,
        cargo_manifest: Option<&[u8]>,
        justfiles: &[(String, Vec<u8>)],
    ) -> Result<CheckCatalog, QualityError> {
        let mut definitions = explicit;
        let mut diagnostics = Vec::new();
        if let Some(manifest) = cargo_manifest {
            cargo::discover(manifest, &[], &mut definitions, &mut diagnostics);
        }
        for (name, bytes) in justfiles {
            just::discover(name, bytes, &mut definitions, &mut diagnostics);
        }
        Ok(canonical_catalog(definitions, diagnostics))
    }
}
