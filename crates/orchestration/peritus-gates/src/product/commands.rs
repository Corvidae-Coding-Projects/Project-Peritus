//! Exact commands for each supported project family.

use std::path::{Path, PathBuf};

use crate::{GateError, GateErrorKind, GateRecoveryAction};

use super::plan::{
    AffectedProject, GateCommandSpec, PRODUCT_MAX_SOURCE_LINES, ProjectKind,
    directory_has_root_python_tests, is_node_test_file,
};

pub(super) fn commands_for(
    workspace_root: &Path,
    project: &AffectedProject,
    changed_paths: &[PathBuf],
) -> Result<Vec<GateCommandSpec>, GateError> {
    let mut commands = vec![source_layout(project)];
    if changed_paths.iter().any(|path| path.starts_with(project.root()) && is_yaml(path)) {
        commands.push(yaml_structure(project));
    }
    let language_commands = match project.kind() {
        ProjectKind::Artifact => artifact_commands(workspace_root, project),
        ProjectKind::Rust => rust_commands(workspace_root, project),
        ProjectKind::Node => node_commands(workspace_root, project),
        ProjectKind::Python => Ok(python_commands(workspace_root, project)),
        ProjectKind::Sqlite => Ok(sqlite_commands(project)),
        ProjectKind::Go => Ok(go_commands(project)),
    }?;
    commands.extend(language_commands);
    Ok(commands)
}

fn is_yaml(path: &Path) -> bool {
    path.extension().and_then(std::ffi::OsStr::to_str).is_some_and(|extension| {
        extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
    })
}

fn source_layout(project: &AffectedProject) -> GateCommandSpec {
    spec(
        "Source layout",
        "peritus-internal",
        vec![
            "source-layout".to_owned(),
            "--max-lines".to_owned(),
            PRODUCT_MAX_SOURCE_LINES.to_string(),
        ],
        project,
    )
}

fn yaml_structure(project: &AffectedProject) -> GateCommandSpec {
    spec("YAML structure", "peritus-internal", vec!["yaml-structure".to_owned()], project)
}

fn rust_commands(
    workspace_root: &Path,
    project: &AffectedProject,
) -> Result<Vec<GateCommandSpec>, GateError> {
    let manifest_path = required_manifest(project)?;
    let manifest = manifest_path.to_string_lossy().into_owned();
    let is_workspace = std::fs::read_to_string(workspace_root.join(manifest_path))
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
    let format = vec![
        "fmt".to_owned(),
        "--manifest-path".to_owned(),
        manifest.clone(),
        "--all".to_owned(),
        "--".to_owned(),
        "--check".to_owned(),
    ];
    let mut commands = vec![
        at_workspace_root(spec("Rust format", "cargo", format, project)),
        at_workspace_root(spec("Rust compile", "cargo", common("check"), project)),
        at_workspace_root(spec("Rust build", "cargo", common("build"), project)),
        at_workspace_root(spec("Rust tests", "cargo", common("test"), project)),
    ];
    let mut clippy = common("clippy");
    clippy.extend(["--".to_owned(), "-D".to_owned(), "warnings".to_owned()]);
    commands.push(at_workspace_root(spec("Rust lint", "cargo", clippy, project)));
    Ok(commands)
}

fn node_commands(
    workspace_root: &Path,
    project: &AffectedProject,
) -> Result<Vec<GateCommandSpec>, GateError> {
    let Some(manifest) = project.manifest() else {
        return conventional_node_commands(workspace_root, project);
    };
    let bytes = std::fs::read(workspace_root.join(manifest))
        .map_err(|_| planning("affected Node manifest is unreadable"))?;
    let package: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| planning("affected Node manifest is invalid"))?;
    let scripts = package.get("scripts").and_then(serde_json::Value::as_object);
    let mut commands = Vec::new();
    for (name, label) in [("build", "Node build"), ("test", "Node tests"), ("lint", "Node lint")] {
        if scripts.is_some_and(|items| items.contains_key(name)) {
            commands.push(spec(label, "npm", vec!["run".to_owned(), name.to_owned()], project));
        }
    }
    Ok(commands)
}

fn conventional_node_commands(
    workspace_root: &Path,
    project: &AffectedProject,
) -> Result<Vec<GateCommandSpec>, GateError> {
    let entries = std::fs::read_dir(workspace_root.join(project.root()))
        .map_err(|_| planning("manifestless Node project root is unreadable"))?;
    let mut tests = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_node_test_file(path))
        .filter_map(|path| path.file_name().and_then(|value| value.to_str()).map(str::to_owned))
        .collect::<Vec<_>>();
    tests.sort();
    if tests.is_empty() {
        return Err(planning("manifestless Node project has no adjacent test file"));
    }
    Ok(tests.into_iter().map(|test| spec("Node tests", "node", vec![test], project)).collect())
}

fn python_commands(workspace_root: &Path, project: &AffectedProject) -> Vec<GateCommandSpec> {
    let mut commands = vec![spec(
        "Python compile",
        "python",
        vec!["-B".to_owned(), "-c".to_owned(), python_syntax_check()],
        project,
    )];
    let root = workspace_root.join(project.root());
    let manifest = || {
        project.manifest().and_then(|path| std::fs::read_to_string(workspace_root.join(path)).ok())
    };
    if root.join("pytest.ini").is_file()
        || root.join("tests").is_dir()
        || directory_has_root_python_tests(&root)
        || manifest().is_some_and(|text| text.contains("pytest"))
    {
        commands.push(spec(
            "Python tests",
            "python",
            vec![
                "-B".to_owned(),
                "-m".to_owned(),
                "pytest".to_owned(),
                "-p".to_owned(),
                "no:cacheprovider".to_owned(),
            ],
            project,
        ));
    }
    if root.join("ruff.toml").is_file()
        || root.join(".ruff.toml").is_file()
        || manifest().is_some_and(|text| text.contains("[tool.ruff"))
    {
        commands.push(spec(
            "Python lint",
            "python",
            vec![
                "-B".to_owned(),
                "-m".to_owned(),
                "ruff".to_owned(),
                "check".to_owned(),
                ".".to_owned(),
            ],
            project,
        ));
    }
    commands
}

fn python_syntax_check() -> String {
    [
        "import ast,pathlib; ",
        "files=(p for p in pathlib.Path('.').rglob('*.py') ",
        "if not any(part.startswith('.') or part in {'build','dist','node_modules','target','vendor'} ",
        "for part in p.parts)); ",
        "[ast.parse(p.read_text(encoding='utf-8'),filename=str(p)) for p in files]",
    ]
    .join("")
}

fn go_commands(project: &AffectedProject) -> Vec<GateCommandSpec> {
    vec![
        spec("Go tests", "go", vec!["test".to_owned(), "./...".to_owned()], project),
        spec("Go lint", "go", vec!["vet".to_owned(), "./...".to_owned()], project),
    ]
}

fn sqlite_commands(project: &AffectedProject) -> Vec<GateCommandSpec> {
    vec![spec(
        "SQLite migration verification",
        "peritus-internal",
        vec!["sqlite-migration".to_owned()],
        project,
    )]
}

fn artifact_commands(
    workspace_root: &Path,
    project: &AffectedProject,
) -> Result<Vec<GateCommandSpec>, GateError> {
    let path = workspace_root.join(required_manifest(project)?);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| planning("artifact workspace manifest is unreadable"))?;
    let value = toml::from_str::<toml::Value>(&text)
        .map_err(|_| planning("artifact workspace manifest is invalid TOML"))?;
    let table =
        value.as_table().ok_or_else(|| planning("artifact workspace manifest must be a table"))?;
    if table.len() != 2
        || table.get("schema_version").and_then(toml::Value::as_integer) != Some(1)
        || table.get("kind").and_then(toml::Value::as_str) != Some("artifact")
    {
        return Err(planning(
            "artifact workspace manifest must contain only schema_version = 1 and kind = \"artifact\"",
        ));
    }
    Ok(vec![spec(
        "Artifact CSV structure",
        "peritus-internal",
        vec!["artifact-csv-structure".to_owned()],
        project,
    )])
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
        current_dir: project.root().to_owned(),
        project: project.clone(),
    }
}

fn planning(detail: &'static str) -> GateError {
    GateError::new(GateErrorKind::Workspace, GateRecoveryAction::CorrectInput, detail)
}

fn required_manifest(project: &AffectedProject) -> Result<&Path, GateError> {
    project.manifest().ok_or_else(|| planning("affected project manifest is missing"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TargetGatePlan;

    #[test]
    fn artifact_manifest_rejects_unknown_or_incompatible_fields() {
        let root = tempfile::tempdir().expect("temporary workspace");
        std::fs::create_dir(root.path().join("out")).expect("output directory");
        std::fs::write(root.path().join("out/result.txt"), "result\n").expect("output artifact");
        for manifest in [
            "schema_version = 2\nkind = \"artifact\"\n",
            "schema_version = 1\nkind = \"artifact\"\nextra = true\n",
        ] {
            std::fs::write(root.path().join("peritus-workspace.toml"), manifest)
                .expect("artifact manifest");
            assert!(
                TargetGatePlan::discover(root.path(), vec![PathBuf::from("out/result.txt")])
                    .is_err()
            );
        }
    }
}
