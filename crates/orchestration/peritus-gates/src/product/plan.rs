//! Changed-path to explicit project-check planning.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::GateError;

use super::commands::commands_for;

/// Supported project families with deterministic production checks.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProjectKind {
    /// Candidate files without a recognized project manifest.
    ///
    /// The host applies only generic file-state and structural checks. It never guesses a build
    /// command or executes an arbitrary `Makefile` target for this fallback project.
    Generic,
    /// Make project with a checked-in `Makefile`.
    Make,
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
    pub(super) optional_when_not_selected: bool,
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

    /// Whether this gate may be an optional observation when its criterion was not selected.
    #[must_use]
    pub const fn optional_when_not_selected(&self) -> bool {
        self.optional_when_not_selected
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

/// Candidate-aware project and command plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetGatePlan {
    changed_paths: Vec<PathBuf>,
    fallback_paths: Vec<PathBuf>,
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
        changed_paths.sort();
        changed_paths.dedup();
        let requested_artifacts = requested_artifacts.iter().collect::<BTreeSet<_>>();
        let mut projects = BTreeSet::new();
        let mut fallback_paths = Vec::new();
        let uncovered_paths = Vec::new();
        for path in &changed_paths {
            let found = nearest_projects(workspace_root, path);
            if found.is_empty() {
                if requested_artifacts.contains(path) {
                    projects.insert(AffectedProject {
                        kind: ProjectKind::Artifact,
                        root: path.parent().unwrap_or_else(|| Path::new("")).to_path_buf(),
                        manifest: None,
                    });
                    continue;
                }
                fallback_paths.push(path.clone());
                continue;
            }
            projects.extend(found);
        }
        if !fallback_paths.is_empty() {
            projects.insert(AffectedProject {
                kind: ProjectKind::Generic,
                root: common_parent(&fallback_paths),
                manifest: None,
            });
        }
        let projects = projects.into_iter().collect::<Vec<_>>();
        let mut commands = Vec::new();
        for project in &projects {
            let project_paths = if project.kind() == ProjectKind::Generic {
                &fallback_paths
            } else {
                &changed_paths
            };
            commands.extend(commands_for(workspace_root, project, project_paths)?);
        }
        Ok(Self { changed_paths, fallback_paths, projects, commands, uncovered_paths })
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

    /// Candidate paths covered by the generic host-owned fallback contract.
    #[must_use]
    pub fn fallback_paths(&self) -> &[PathBuf] {
        &self.fallback_paths
    }

    /// Exact commands required for acceptance.
    #[must_use]
    pub fn commands(&self) -> &[GateCommandSpec] {
        &self.commands
    }

    /// Marks conventional test suites as unselected for an explicitly requested real-testing
    /// workflow. The executor must retain a truthful observation for every excluded command;
    /// exclusion is not a successful test result. Other checks remain required.
    #[must_use]
    pub fn with_test_suites_unselected(mut self) -> Self {
        for command in &mut self.commands {
            if command.label.to_ascii_lowercase().contains("test") {
                command.optional_when_not_selected = true;
            }
        }
        self
    }

    /// Retains conventional qualification commands as unselected observations when the caller
    /// explicitly chooses real-interface acceptance instead of those suites. Internal artifact
    /// and source checks remain required; missing observations cannot satisfy the plan.
    #[must_use]
    pub fn with_conventional_checks_unselected(mut self) -> Self {
        for command in &mut self.commands {
            if command.program != "peritus-internal" {
                command.optional_when_not_selected = true;
            }
        }
        self
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

fn common_parent(paths: &[PathBuf]) -> PathBuf {
    let mut common =
        paths.first().and_then(|path| path.parent()).unwrap_or_else(|| Path::new("")).to_path_buf();
    while !paths.iter().all(|path| path.starts_with(&common)) {
        if !common.pop() {
            break;
        }
    }
    common
}

fn nearest_projects(workspace_root: &Path, changed: &Path) -> Vec<AffectedProject> {
    let standalone_python = standalone_python_project(changed);
    let mut relative = changed.parent().unwrap_or_else(|| Path::new(""));
    loop {
        let absolute = workspace_root.join(relative);
        let mut found = [
            (ProjectKind::Artifact, "peritus-workspace.toml"),
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
        if found.is_empty()
            && let Some(marker) = ["GNUmakefile", "makefile", "Makefile"]
                .into_iter()
                .find(|marker| absolute.join(marker).is_file())
        {
            found.push(AffectedProject {
                kind: ProjectKind::Make,
                root: relative.to_path_buf(),
                manifest: Some(relative.join(marker)),
            });
        }
        if !found.is_empty() {
            if let Some(python) = &standalone_python
                && !found.iter().any(|project| project.kind == ProjectKind::Python)
            {
                if found.iter().all(|project| project.kind == ProjectKind::Artifact) {
                    return vec![python.clone()];
                }
                found.push(python.clone());
            }
            return found;
        }
        let Some(parent) = relative.parent() else { break };
        if parent == relative {
            break;
        }
        relative = parent;
    }
    standalone_python.into_iter().collect()
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
