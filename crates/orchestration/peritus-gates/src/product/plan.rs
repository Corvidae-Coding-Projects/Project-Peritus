//! Changed-path to explicit project-check planning.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::GateError;

use super::commands::commands_for;

/// Hard source-file ceiling enforced by the built-in production workflow.
pub const PRODUCT_MAX_SOURCE_LINES: usize = 500;

/// Supported project families with deterministic production checks.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProjectKind {
    /// Explicit general artifact workspace whose semantic oracle is host-owned.
    Artifact,
    /// Cargo package or workspace.
    Rust,
    /// Node package.
    Node,
    /// Python project.
    Python,
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

    /// Manifest relative to the managed workspace, when the project declares one.
    ///
    /// Conventional manifestless Python projects are represented without inventing a path.
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
    ) -> Result<Self, GateError> {
        changed_paths.sort();
        changed_paths.dedup();
        let mut projects = BTreeSet::new();
        let mut uncovered_paths = Vec::new();
        for path in &changed_paths {
            let found = nearest_projects(workspace_root, path);
            if found.is_empty() {
                uncovered_paths.push(path.clone());
                continue;
            }
            projects.extend(found);
        }
        let projects = projects.into_iter().collect::<Vec<_>>();
        let mut commands = Vec::new();
        for project in &projects {
            commands.extend(commands_for(workspace_root, project)?);
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

fn nearest_projects(workspace_root: &Path, changed: &Path) -> Vec<AffectedProject> {
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
        if !found.is_empty() {
            return found;
        }
        let Some(parent) = relative.parent() else { break };
        if parent == relative {
            break;
        }
        relative = parent;
    }
    Vec::new()
}

fn conventional_python_tests(absolute: &Path, relative: &Path, changed: &Path) -> bool {
    let tests = absolute.join("tests");
    let belongs_to_python = changed.extension().is_some_and(|extension| extension == "py")
        || changed.strip_prefix(relative).is_ok_and(|within| {
            within.components().next().is_some_and(|part| part.as_os_str() == "tests")
        });
    if !tests.is_dir() || !belongs_to_python {
        return false;
    }
    std::fs::read_dir(tests).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|extension| extension == "py"))
    })
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
