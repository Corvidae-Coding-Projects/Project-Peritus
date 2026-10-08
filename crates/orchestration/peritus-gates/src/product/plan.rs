//! Changed-path to explicit project-check planning.

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::GateError;

use super::commands::commands_for;

/// Supported project families with deterministic production checks.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProjectKind {
    /// General artifact output whose semantic oracle is host-owned.
    ///
    /// A checked-in `peritus-workspace.toml` or an exact caller-requested output can establish
    /// this project kind. The optional manifest distinguishes those two host-observed contracts.
    Artifact,
    /// Cargo package or workspace.
    Rust,
    /// Node package.
    Node,
    /// Python project.
    Python,
    /// Conventional `SQLite` schema and migration workspace.
    Sqlite,
    /// Go module.
    Go,
}

/// Network authority required by one exact gate command.
///
/// A denied command still runs in a native sandbox: this value describes the contract that the
/// backend must enforce, not a promise inferred from the executable name or script contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateNetworkPolicy {
    /// The complete process tree must have no network access.
    Denied,
    /// The gate is an explicitly network-dependent check and requires live Network capability.
    ExplicitAuthority,
    /// The trusted service host selected one exact managed-egress grant.
    ManagedAuthority {
        /// Canonical identity of the complete host grant.
        grant_digest: Sha256Digest,
    },
}

impl GateNetworkPolicy {
    const fn tag(self) -> u8 {
        match self {
            Self::Denied => 1,
            Self::ExplicitAuthority => 2,
            Self::ManagedAuthority { .. } => 3,
        }
    }

    /// Whether this command requires the host's live Network capability.
    #[must_use]
    pub const fn requires_authority(self) -> bool {
        matches!(self, Self::ExplicitAuthority | Self::ManagedAuthority { .. })
    }

    /// Returns the exact managed grant identity, when the host selected one.
    #[must_use]
    pub const fn managed_grant(self) -> Option<Sha256Digest> {
        match self {
            Self::ManagedAuthority { grant_digest } => Some(grant_digest),
            Self::Denied | Self::ExplicitAuthority => None,
        }
    }
}

/// One exact project implicated by candidate paths.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AffectedProject {
    kind: ProjectKind,
    root: PathBuf,
    manifest: Option<PathBuf>,
}

impl AffectedProject {
    /// Project family.
    #[must_use]
    pub const fn kind(&self) -> ProjectKind {
        self.kind
    }

    /// Root relative to the managed workspace.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Manifest or conventional project anchor relative to the managed workspace.
    ///
    /// Manifestless Python and Node projects are represented without inventing a path.
    #[must_use]
    pub fn manifest(&self) -> Option<&Path> {
        self.manifest.as_deref()
    }
}

/// Structured argv gate tied to one affected project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateCommandSpec {
    pub(super) label: String,
    pub(super) program: String,
    pub(super) arguments: Vec<String>,
    pub(super) current_dir: PathBuf,
    pub(super) project: AffectedProject,
    pub(super) network: GateNetworkPolicy,
}

impl GateCommandSpec {
    /// Stable human-readable command purpose.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Executable name.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Exact argument vector.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Working directory relative to the managed workspace.
    #[must_use]
    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    /// Exact affected project this command covers.
    #[must_use]
    pub const fn project(&self) -> &AffectedProject {
        &self.project
    }

    /// Native network contract required for this command and all of its descendants.
    #[must_use]
    pub const fn network_policy(&self) -> GateNetworkPolicy {
        self.network
    }

    /// Canonical identity used by the trusted host to select optional managed egress.
    ///
    /// This is exactly the legacy denied-network command identity. Selecting a grant therefore
    /// cannot change what command the grant was configured for, while commands that remain
    /// deny-all retain their accepted version-two identity byte-for-byte.
    #[must_use]
    pub fn base_identity(&self) -> Sha256Digest {
        self.legacy_identity(GateNetworkPolicy::Denied)
    }

    /// Binds one trusted managed-network grant to this exact structured command.
    #[must_use]
    pub fn with_managed_network(mut self, grant_digest: Sha256Digest) -> Self {
        self.network = GateNetworkPolicy::ManagedAuthority { grant_digest };
        self
    }

    /// Canonical identity of the exact structured command and its declared authority policy.
    ///
    /// The product runner combines this with the candidate and requirement identities before
    /// reserving a retained gate operation.
    #[must_use]
    pub fn identity(&self) -> Sha256Digest {
        if let GateNetworkPolicy::ManagedAuthority { grant_digest } = self.network {
            let mut hasher = Sha256::new();
            hasher.update(b"peritus-target-gate-command-v3\0");
            hasher.update(self.base_identity().as_bytes());
            hasher.update(grant_digest.as_bytes());
            return Sha256Digest::new(hasher.finalize().into());
        }
        self.legacy_identity(self.network)
    }

    fn legacy_identity(&self, network: GateNetworkPolicy) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus-target-gate-command-v2\0");
        hash_bytes(&mut hasher, self.label.as_bytes());
        hash_bytes(&mut hasher, self.program.as_bytes());
        hasher.update(u64::try_from(self.arguments.len()).unwrap_or(u64::MAX).to_le_bytes());
        for argument in &self.arguments {
            hash_bytes(&mut hasher, argument.as_bytes());
        }
        hash_native_os_str(&mut hasher, self.current_dir.as_os_str());
        hasher.update([project_kind_tag(self.project.kind), network.tag()]);
        hash_native_os_str(&mut hasher, self.project.root.as_os_str());
        match &self.project.manifest {
            Some(manifest) => {
                hasher.update([1]);
                hash_native_os_str(&mut hasher, manifest.as_os_str());
            }
            None => hasher.update([0]),
        }
        Sha256Digest::new(hasher.finalize().into())
    }

    /// Shell-like display form for user evidence. Execution still uses structured argv.
    #[must_use]
    pub fn display(&self) -> String {
        let command = std::iter::once(self.program.as_str())
            .chain(self.arguments.iter().map(String::as_str))
            .map(quote_argument)
            .collect::<Vec<_>>()
            .join(" ");
        if self.current_dir.as_os_str().is_empty() {
            command
        } else {
            format!("(cd {} && {command})", quote_argument(&self.current_dir.to_string_lossy()))
        }
    }
}

const fn project_kind_tag(kind: ProjectKind) -> u8 {
    match kind {
        ProjectKind::Artifact => 1,
        ProjectKind::Rust => 2,
        ProjectKind::Node => 3,
        ProjectKind::Python => 4,
        ProjectKind::Sqlite => 5,
        ProjectKind::Go => 6,
    }
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

fn hash_native_os_str(hasher: &mut Sha256, value: &OsStr) {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;

        hasher.update([1]);
        hash_bytes(hasher, value.as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;

        hasher.update([2]);
        let units = value.encode_wide().collect::<Vec<_>>();
        hasher.update(u64::try_from(units.len()).unwrap_or(u64::MAX).to_le_bytes());
        for unit in units {
            hasher.update(unit.to_be_bytes());
        }
    }
}

/// Candidate-aware project and command plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetGatePlan {
    changed_paths: Vec<PathBuf>,
    projects: Vec<AffectedProject>,
    commands: Vec<GateCommandSpec>,
    uncovered_paths: Vec<PathBuf>,
}

impl TargetGatePlan {
    /// Discovers the nearest project manifests for every changed path and plans explicit checks.
    ///
    /// # Errors
    /// Returns a gate planning error when a discovered manifest cannot be read or parsed.
    pub fn discover(
        workspace_root: &Path,
        mut changed_paths: Vec<PathBuf>,
        requested_artifacts: &[PathBuf],
    ) -> Result<Self, GateError> {
        // The global declaration governs admission even when a nested language manifest wins
        // nearest-project discovery. It must never be hidden by that navigation decision.
        let _ = super::WorkspaceProductScope::read(workspace_root)?;
        changed_paths.sort();
        changed_paths.dedup();
        let requested_artifacts = requested_artifacts.iter().collect::<BTreeSet<_>>();
        let mut projects = BTreeSet::new();
        let mut uncovered_paths = Vec::new();
        for path in &changed_paths {
            let found = nearest_projects(workspace_root, path)?;
            if found.is_empty() {
                if requested_artifacts.contains(path) {
                    projects.insert(AffectedProject {
                        kind: ProjectKind::Artifact,
                        root: path.parent().unwrap_or_else(|| Path::new("")).to_path_buf(),
                        manifest: None,
                    });
                    continue;
                }
                uncovered_paths.push(path.clone());
                continue;
            }
            projects.extend(found);
        }
        let projects = projects.into_iter().collect::<Vec<_>>();
        uncovered_paths.retain(|path| {
            !projects.iter().any(|project| adjacent_to_manifestless_project(path, project))
        });
        let mut commands = Vec::new();
        for project in &projects {
            commands.extend(commands_for(workspace_root, project, &changed_paths)?);
        }
        Ok(Self { changed_paths, projects, commands, uncovered_paths })
    }

    /// Exact candidate paths compared with the pre-run baseline.
    #[must_use]
    pub fn changed_paths(&self) -> &[PathBuf] {
        &self.changed_paths
    }

    /// Affected project set.
    #[must_use]
    pub fn projects(&self) -> &[AffectedProject] {
        &self.projects
    }

    /// Exact commands required for acceptance.
    #[must_use]
    pub fn commands(&self) -> &[GateCommandSpec] {
        &self.commands
    }

    /// Changed paths for which no executable project contract was found.
    #[must_use]
    pub fn uncovered_paths(&self) -> &[PathBuf] {
        &self.uncovered_paths
    }

    /// Whether every candidate path has an affected project and every project has checks.
    #[must_use]
    pub fn has_complete_coverage(&self) -> bool {
        !self.changed_paths.is_empty()
            && self.uncovered_paths.is_empty()
            && !self.projects.is_empty()
            && !self.commands.is_empty()
            && self
                .projects
                .iter()
                .all(|project| self.commands.iter().any(|command| command.project() == project))
    }
}

fn adjacent_to_manifestless_project(path: &Path, project: &AffectedProject) -> bool {
    project.manifest().is_none()
        && matches!(project.kind(), ProjectKind::Python | ProjectKind::Node)
        && path.parent().unwrap_or_else(|| Path::new("")) == project.root()
}

fn nearest_projects(workspace_root: &Path, changed: &Path) -> Result<Vec<AffectedProject>, GateError> {
    let standalone_python = standalone_python_project(changed);
    let mut relative = changed.parent().unwrap_or_else(|| Path::new(""));
    loop {
        let absolute = workspace_root.join(relative);
        let scope = super::WorkspaceProductScope::read(&absolute)?;
        let mut found = [
            (ProjectKind::Rust, "Cargo.toml"),
            (ProjectKind::Node, "package.json"),
            (ProjectKind::Python, "pyproject.toml"),
            (ProjectKind::Python, "pytest.ini"),
            (ProjectKind::Go, "go.mod"),
        ]
        .into_iter()
        .filter(|(_, marker)| absolute.join(marker).is_file())
        .map(|(kind, marker)| AffectedProject {
            kind,
            root: relative.to_path_buf(),
            manifest: Some(relative.join(marker)),
        })
        .collect::<Vec<_>>();
        if scope == super::WorkspaceProductScope::Artifact {
            found.push(AffectedProject {
                kind: ProjectKind::Artifact,
                root: relative.to_path_buf(),
                manifest: Some(relative.join("peritus-workspace.toml")),
            });
        }
        if !found.iter().any(|project| project.kind == ProjectKind::Python)
            && conventional_python_tests(&absolute, relative, changed)
        {
            found.push(AffectedProject {
                kind: ProjectKind::Python,
                root: relative.to_path_buf(),
                manifest: None,
            });
        }
        if !found.iter().any(|project| project.kind == ProjectKind::Node)
            && conventional_node_tests(&absolute, changed)
        {
            found.push(AffectedProject {
                kind: ProjectKind::Node,
                root: relative.to_path_buf(),
                manifest: None,
            });
        }
        if conventional_sqlite_migration(&absolute, changed) {
            found.push(AffectedProject {
                kind: ProjectKind::Sqlite,
                root: relative.to_path_buf(),
                manifest: Some(relative.join("schema.sql")),
            });
        }
        if !found.is_empty() {
            if let Some(python) = &standalone_python
                && !found.iter().any(|project| project.kind == ProjectKind::Python)
            {
                if found.iter().all(|project| project.kind == ProjectKind::Artifact) {
                    return Ok(vec![python.clone()]);
                }
                found.push(python.clone());
            }
            return Ok(found);
        }
        let Some(parent) = relative.parent() else { break };
        if parent == relative {
            break;
        }
        relative = parent;
    }
    Ok(standalone_python.into_iter().collect())
}

fn standalone_python_project(changed: &Path) -> Option<AffectedProject> {
    let parent = changed.parent()?;
    let is_test_support = changed.components().any(|component| component.as_os_str() == "tests")
        || changed.file_name().is_some_and(|name| name == "conftest.py")
        || python_test_name(changed);
    if changed.extension().is_some_and(|extension| extension == "py") && !is_test_support {
        return Some(AffectedProject {
            kind: ProjectKind::Python,
            root: parent.to_path_buf(),
            manifest: None,
        });
    }
    None
}

fn conventional_sqlite_migration(root: &Path, changed: &Path) -> bool {
    root.join("schema.sql").is_file()
        && root.join("migration.sql").is_file()
        && sqlite_candidate_path(changed)
}

fn sqlite_candidate_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "sql")
        || path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| {
            ["migration", "postcheck", "preflight", "rollback"]
                .iter()
                .any(|term| stem.contains(term))
        })
}

fn conventional_python_tests(absolute: &Path, relative: &Path, changed: &Path) -> bool {
    let tests = absolute.join("tests");
    let has_tests_directory = tests.is_dir()
        && std::fs::read_dir(tests).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| entry.path().extension().is_some_and(|extension| extension == "py"))
        });
    let has_root_tests = absolute.file_name().is_none_or(|name| name != "tests")
        && directory_has_root_python_tests(absolute);
    changed.strip_prefix(relative).is_ok() && (has_tests_directory || has_root_tests)
}

pub(super) fn directory_has_root_python_tests(directory: &Path) -> bool {
    std::fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| is_root_python_test(&entry.path()))
    })
}

fn is_root_python_test(path: &Path) -> bool {
    if !path.is_file() || path.extension().and_then(|extension| extension.to_str()) != Some("py") {
        return false;
    }
    python_test_name(path)
}

fn python_test_name(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.starts_with("test_") || stem.ends_with("_test"))
}

fn conventional_node_tests(absolute: &Path, changed: &Path) -> bool {
    matches!(changed.extension().and_then(|value| value.to_str()), Some("js" | "cjs" | "mjs"))
        && std::fs::read_dir(absolute).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| is_node_test_file(&entry.path()))
        })
}

pub(super) fn is_node_test_file(path: &Path) -> bool {
    path.file_name().and_then(|value| value.to_str()).is_some_and(|name| {
        [".test.js", ".spec.js", ".test.cjs", ".spec.cjs", ".test.mjs", ".spec.mjs"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
    })
}

fn quote_argument(value: &str) -> String {
    if value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte)) {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

#[cfg(test)]
mod tests;
