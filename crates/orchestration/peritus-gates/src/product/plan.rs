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
    if !tests.is_dir()
        || !changed.strip_prefix(relative).is_ok_and(|within| {
            within.components().next().is_some_and(|part| part.as_os_str() == "tests")
        })
    {
        return false;
    }
    std::fs::read_dir(tests).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|extension| extension == "py"))
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
mod tests {
    use super::*;

    #[test]
    fn rust_plan_builds_the_exact_nested_package() {
        let temporary = tempfile::tempdir().expect("temporary workspace");
        let project = temporary.path().join("game");
        std::fs::create_dir_all(project.join("src")).expect("nested package directory");
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .expect("nested package manifest");
        std::fs::write(project.join("src/main.rs"), "fn main() { println!(\"game\"); }\n")
            .expect("nested package source");

        let plan =
            TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("game/src/main.rs")])
                .expect("exact target plan");

        let build = plan
            .commands()
            .iter()
            .find(|command| command.label() == "Rust build")
            .expect("Rust build gate");
        let format = plan
            .commands()
            .iter()
            .find(|command| command.label() == "Rust format")
            .expect("Rust format gate");
        assert_eq!(format.program(), "cargo");
        assert_eq!(format.current_dir(), Path::new(""));
        assert_eq!(
            format.arguments(),
            ["fmt", "--manifest-path", "game/Cargo.toml", "--all", "--", "--check"]
        );
        assert_eq!(build.program(), "cargo");
        assert_eq!(build.current_dir(), Path::new(""));
        assert_eq!(
            build.arguments(),
            [
                "build",
                "--locked",
                "--all-targets",
                "--all-features",
                "--manifest-path",
                "game/Cargo.toml",
                "--workspace",
            ]
        );
    }

    #[test]
    fn explicit_artifact_workspace_covers_general_outputs() {
        let temporary = tempfile::tempdir().expect("temporary workspace");
        std::fs::write(
            temporary.path().join("peritus-workspace.toml"),
            "schema_version = 1\nkind = \"artifact\"\n",
        )
        .expect("artifact workspace marker");
        std::fs::create_dir(temporary.path().join("out")).expect("output directory");
        std::fs::write(temporary.path().join("out/result.txt"), "result\n")
            .expect("output artifact");

        let plan =
            TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("out/result.txt")])
                .expect("artifact plan");

        assert!(plan.has_complete_coverage());
        assert!(plan.uncovered_paths().is_empty());
        assert_eq!(plan.projects()[0].kind(), ProjectKind::Artifact);
        assert_eq!(plan.commands().len(), 2);
        assert_eq!(plan.commands()[0].label(), "Source layout");
        assert_eq!(plan.commands()[1].label(), "Artifact CSV structure");
    }

    #[test]
    fn manifestless_python_tests_use_their_nearest_conventional_project() {
        let temporary = tempfile::tempdir().expect("temporary workspace");
        std::fs::write(
            temporary.path().join("peritus-workspace.toml"),
            "schema_version = 1\nkind = \"artifact\"\n",
        )
        .expect("artifact workspace marker");
        let project = temporary.path().join("in/ordercalc");
        std::fs::create_dir_all(project.join("ordercalc")).expect("package directory");
        std::fs::create_dir_all(project.join("tests")).expect("test directory");
        std::fs::write(project.join("ordercalc/__init__.py"), "").expect("package marker");
        std::fs::write(project.join("tests/test_pricing.py"), "def test_price():\n    pass\n")
            .expect("test module");
        std::fs::write(project.join("tests/TEST_INTENT.md"), "pricing contract\n")
            .expect("test documentation");

        let plan = TargetGatePlan::discover(
            temporary.path(),
            vec![
                PathBuf::from("in/ordercalc/tests/TEST_INTENT.md"),
                PathBuf::from("in/ordercalc/tests/test_pricing.py"),
            ],
        )
        .expect("manifestless Python plan");

        assert!(plan.has_complete_coverage());
        assert_eq!(plan.projects().len(), 1);
        assert_eq!(plan.projects()[0].kind(), ProjectKind::Python);
        assert_eq!(plan.projects()[0].root(), Path::new("in/ordercalc"));
        assert_eq!(plan.projects()[0].manifest(), None);
        assert!(plan.commands().iter().any(|command| command.label() == "Python compile"));
        assert!(plan.commands().iter().any(|command| command.label() == "Python tests"));
        assert!(plan.commands().iter().all(|command| {
            command.current_dir() == Path::new("in/ordercalc")
                && command.project().kind() == ProjectKind::Python
        }));
    }
}
