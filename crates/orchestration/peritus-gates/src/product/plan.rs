//! Changed-path to explicit project-check planning.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{GateError, GateErrorKind, GateRecoveryAction};

/// Supported project families with deterministic production checks.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProjectKind {
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
    manifest: PathBuf,
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

    /// Manifest relative to the managed workspace.
    #[must_use]
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }
}

/// Structured argv gate tied to one affected project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateCommandSpec {
    label: String,
    program: String,
    arguments: Vec<String>,
    current_dir: PathBuf,
    project: AffectedProject,
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
        let found = [
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
            manifest: relative.join(marker),
        })
        .collect::<Vec<_>>();
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

fn commands_for(
    workspace_root: &Path,
    project: &AffectedProject,
) -> Result<Vec<GateCommandSpec>, GateError> {
    match project.kind {
        ProjectKind::Rust => Ok(rust_commands(workspace_root, project)),
        ProjectKind::Node => node_commands(workspace_root, project),
        ProjectKind::Python => Ok(python_commands(workspace_root, project)),
        ProjectKind::Go => Ok(go_commands(project)),
    }
}

fn rust_commands(workspace_root: &Path, project: &AffectedProject) -> Vec<GateCommandSpec> {
    let manifest = project.manifest.to_string_lossy().into_owned();
    let is_workspace = std::fs::read_to_string(workspace_root.join(&project.manifest))
        .is_ok_and(|text| text.lines().any(|line| line.trim() == "[workspace]"));
    let common = |verb: &str| {
        let mut arguments = vec![
            verb.to_owned(),
            "--locked".to_owned(),
            "--all-targets".to_owned(),
            "--all-features".to_owned(),
            "--manifest-path".to_owned(),
            manifest.clone(),
        ];
        if is_workspace {
            arguments.push("--workspace".to_owned());
        }
        arguments
    };
    let at_workspace_root = |mut command: GateCommandSpec| {
        command.current_dir = PathBuf::new();
        command
    };
    let mut commands =
        vec![at_workspace_root(spec("Rust compile", "cargo", common("check"), project))];
    commands.push(at_workspace_root(spec("Rust tests", "cargo", common("test"), project)));
    let mut clippy = common("clippy");
    clippy.extend(["--".to_owned(), "-D".to_owned(), "warnings".to_owned()]);
    commands.push(at_workspace_root(spec("Rust lint", "cargo", clippy, project)));
    commands
}

fn node_commands(
    workspace_root: &Path,
    project: &AffectedProject,
) -> Result<Vec<GateCommandSpec>, GateError> {
    let bytes = std::fs::read(workspace_root.join(&project.manifest)).map_err(|_| planning())?;
    let package: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| planning())?;
    let scripts = package.get("scripts").and_then(serde_json::Value::as_object);
    let mut commands = Vec::new();
    for (name, label) in [("build", "Node build"), ("test", "Node tests"), ("lint", "Node lint")] {
        if scripts.is_some_and(|items| items.contains_key(name)) {
            commands.push(spec(label, "npm", vec!["run".to_owned(), name.to_owned()], project));
        }
    }
    Ok(commands)
}

fn python_commands(workspace_root: &Path, project: &AffectedProject) -> Vec<GateCommandSpec> {
    let mut commands = vec![spec(
        "Python compile",
        "python",
        vec!["-m".to_owned(), "compileall".to_owned(), "-q".to_owned(), ".".to_owned()],
        project,
    )];
    let root = workspace_root.join(&project.root);
    if root.join("pytest.ini").is_file()
        || root.join("tests").is_dir()
        || std::fs::read_to_string(workspace_root.join(&project.manifest))
            .is_ok_and(|text| text.contains("pytest"))
    {
        commands.push(spec(
            "Python tests",
            "python",
            vec!["-m".to_owned(), "pytest".to_owned()],
            project,
        ));
    }
    if root.join("ruff.toml").is_file()
        || root.join(".ruff.toml").is_file()
        || std::fs::read_to_string(workspace_root.join(&project.manifest))
            .is_ok_and(|text| text.contains("[tool.ruff"))
    {
        commands.push(spec(
            "Python lint",
            "python",
            vec!["-m".to_owned(), "ruff".to_owned(), "check".to_owned(), ".".to_owned()],
            project,
        ));
    }
    commands
}

fn go_commands(project: &AffectedProject) -> Vec<GateCommandSpec> {
    vec![
        spec("Go tests", "go", vec!["test".to_owned(), "./...".to_owned()], project),
        spec("Go lint", "go", vec!["vet".to_owned(), "./...".to_owned()], project),
    ]
}

fn spec(
    label: &str,
    program: &str,
    arguments: Vec<String>,
    project: &AffectedProject,
) -> GateCommandSpec {
    GateCommandSpec {
        label: label.to_owned(),
        program: program.to_owned(),
        arguments,
        current_dir: project.root.clone(),
        project: project.clone(),
    }
}

fn planning() -> GateError {
    GateError::new(
        GateErrorKind::Workspace,
        GateRecoveryAction::CorrectInput,
        "affected project manifest is unreadable",
    )
}

fn quote_argument(value: &str) -> String {
    if value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte)) {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}
