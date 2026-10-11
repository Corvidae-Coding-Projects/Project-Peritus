//! Explicit-plan `SQLite` migration checks on isolated copies of the declared initial database.

use std::{
    fs,
    io::Read as _,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use peritus_gates::{GateExecutionRecord, GateObservation};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};

use super::{GateOutcome, cancellation::GateCancellation};

mod assertion;
use assertion::{SqliteAssertion, check_assertion, parse_assertion};

#[derive(Debug)]
struct SqliteMigrationPlan {
    initial_database: PathBuf,
    migrations: Vec<PathBuf>,
    repeatability: bool,
    rollback: Option<PathBuf>,
    assertions: Vec<SqliteAssertion>,
}

enum PlanObservation {
    Complete(SqliteMigrationPlan),
    Incomplete(String),
    Absent,
}

pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[PathBuf],
    command: String,
    request_context: &str,
    cancellation: &GateCancellation,
) -> GateOutcome {
    let observation = parse_plan(workspace_root, project_root, changed_paths, request_context);
    let plan = match observation {
        PlanObservation::Complete(plan) => plan,
        PlanObservation::Incomplete(reason) => {
            return GateOutcome::Required(GateExecutionRecord {
                command,
                label: "SQLite migration verification".to_owned(),
                exit_code: None,
                output: format!("SQLite migration verification: NOT EVALUATED: {reason}\n"),
            });
        }
        PlanObservation::Absent => {
            return GateOutcome::Optional(GateObservation {
                command,
                label: "SQLite migration verification".to_owned(),
                output: "NOT EVALUATED: no SQLite migration acceptance check was selected by the request".to_owned(),
            });
        }
    };
    match verify(workspace_root, &plan, cancellation) {
        Ok(phases) => GateOutcome::Required(GateExecutionRecord {
            command,
            label: "SQLite migration verification".to_owned(),
            exit_code: Some(0),
            output: format!("{}\nSQLite migration verification: PASS\n", phases.join("\n")),
        }),
        Err(detail) if cancellation.is_cancelled() => GateOutcome::Required(GateExecutionRecord {
            command,
            label: "SQLite migration verification".to_owned(),
            exit_code: None,
            output: format!("SQLite migration verification: NOT EVALUATED: {detail}\n"),
        }),
        Err(detail) => GateOutcome::Required(GateExecutionRecord {
            command,
            label: "SQLite migration verification".to_owned(),
            exit_code: Some(1),
            output: format!("SQLite migration verification: FAIL: {detail}\n"),
        }),
    }
}

fn parse_plan(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[PathBuf],
    request_context: &str,
) -> PlanObservation {
    let mut initial = None;
    let mut migrations = Vec::new();
    let mut repeatability = false;
    let mut rollback = None;
    let mut assertions = Vec::new();
    let mut saw_declaration = false;
    for line in request_context.lines() {
        let line = line.trim().trim_start_matches(['-', '*']).trim();
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "sqlite initial database" | "initial sqlite database" => {
                saw_declaration = true;
                match resolve_declared_path(workspace_root, project_root, value) {
                    Ok(path) => initial = Some(path),
                    Err(error) => return PlanObservation::Incomplete(error),
                }
            }
            "sqlite migration" | "sqlite migration path" => {
                saw_declaration = true;
                match resolve_declared_path(workspace_root, project_root, value) {
                    Ok(path) => migrations.push(path),
                    Err(error) => return PlanObservation::Incomplete(error),
                }
            }
            "sqlite repeatability" => {
                saw_declaration = true;
                match value
                    .trim_matches(|character| matches!(character, '`' | '\'' | '"'))
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "required" | "yes" | "true" => repeatability = true,
                    "not required" | "no" | "false" => repeatability = false,
                    other => {
                        return PlanObservation::Incomplete(format!(
                            "unsupported repeatability selection {other:?}"
                        ));
                    }
                }
            }
            "sqlite rollback" | "sqlite rollback path" => {
                saw_declaration = true;
                match resolve_declared_path(workspace_root, project_root, value) {
                    Ok(path) => rollback = Some(path),
                    Err(error) => return PlanObservation::Incomplete(error),
                }
            }
            "sqlite assertion" | "sqlite postcheck assertion" => {
                saw_declaration = true;
                match parse_assertion(value) {
                    Ok(assertion) => assertions.push(assertion),
                    Err(error) => return PlanObservation::Incomplete(error),
                }
            }
            _ => {}
        }
    }
    if !saw_declaration {
        return PlanObservation::Absent;
    }
    let Some(initial_database) = initial else {
        return PlanObservation::Incomplete(
            "the SQLite contract does not declare an initial database path".to_owned(),
        );
    };
    if migrations.is_empty() {
        return PlanObservation::Incomplete(
            "the SQLite contract does not declare ordered migration paths".to_owned(),
        );
    }
    if migrations.iter().any(|migration| !changed_paths.contains(migration)) {
        return PlanObservation::Incomplete(
            "every declared SQLite migration must be an exact changed candidate path".to_owned(),
        );
    }
    if !initial_database.starts_with(project_root)
        || migrations.iter().any(|path| !path.starts_with(project_root))
        || rollback.as_ref().is_some_and(|path| !path.starts_with(project_root))
    {
        return PlanObservation::Incomplete(
            "all declared SQLite paths must remain inside the selected project root".to_owned(),
        );
    }
    for path in std::iter::once(&initial_database).chain(&migrations).chain(rollback.iter()) {
        if let Err(error) = require_regular_file(workspace_root, path) {
            return PlanObservation::Incomplete(error);
        }
    }
    PlanObservation::Complete(SqliteMigrationPlan {
        initial_database,
        migrations,
        repeatability,
        rollback,
        assertions,
    })
}

fn resolve_declared_path(
    workspace_root: &Path,
    project_root: &Path,
    raw: &str,
) -> Result<PathBuf, String> {
    let value = unquote(raw.trim()).unwrap_or_else(|| raw.trim());
    let candidate = PathBuf::from(value);
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!(
            "declared SQLite path {value:?} contains a parent or platform-prefix component"
        ));
    }
    let path = if candidate.is_absolute() {
        candidate
            .strip_prefix(workspace_root)
            .map_err(|_| {
                format!("declared SQLite path {value:?} is outside the managed workspace")
            })?
            .to_path_buf()
    } else {
        candidate.components().filter(|component| !matches!(component, Component::CurDir)).fold(
            PathBuf::new(),
            |mut path, component| {
                path.push(component.as_os_str());
                path
            },
        )
    };
    if path.as_os_str().is_empty() || !workspace_root.join(&path).starts_with(workspace_root) {
        return Err(format!("declared SQLite path {value:?} does not resolve under the workspace"));
    }
    let canonical_root = fs::canonicalize(workspace_root.join(project_root))
        .map_err(|error| format!("inspect SQLite project root: {error}"))?;
    let canonical_path = fs::canonicalize(workspace_root.join(&path))
        .map_err(|error| format!("inspect declared SQLite path {}: {error}", path.display()))?;
    if !canonical_path.starts_with(canonical_root) {
        return Err(format!(
            "declared SQLite path {} resolves outside the selected project root",
            path.display()
        ));
    }
    Ok(path)
}

fn unquote(value: &str) -> Option<&str> {
    let value = value.trim();
    let first = value.chars().next()?;
    let last = value.chars().next_back()?;
    (value.len() >= 2 && matches!(first, '`' | '\'' | '"') && first == last)
        .then(|| &value[first.len_utf8()..value.len() - last.len_utf8()])
}

fn require_regular_file(root: &Path, path: &Path) -> Result<(), String> {
    let full_path = root.join(path);
    let metadata = fs::symlink_metadata(&full_path)
        .map_err(|error| format!("inspect declared SQLite file {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("declared SQLite path {} is not a regular file", path.display()));
    }
    Ok(())
}

fn verify(
    root: &Path,
    plan: &SqliteMigrationPlan,
    cancellation: &GateCancellation,
) -> Result<Vec<String>, String> {
    let initial_path = root.join(&plan.initial_database);
    let initial = Connection::open_with_flags(initial_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| {
            format!("open declared initial database {}: {error}", plan.initial_database.display())
        })?;
    let mut phases = Vec::new();

    let forward = isolated_copy(&initial, cancellation)?;
    execute_migrations(root, &forward, &plan.migrations, cancellation)?;
    check_foreign_keys(&forward, "migration")?;
    phases.push(format!("Declared migration run: PASS ({} file(s))", plan.migrations.len()));

    if plan.repeatability {
        execute_migrations(root, &forward, &plan.migrations, cancellation)?;
        check_foreign_keys(&forward, "repeatability")?;
        phases.push("Requested repeatability: PASS".to_owned());
    }
    if !plan.assertions.is_empty() {
        for (index, assertion) in plan.assertions.iter().enumerate() {
            cancellation_check(cancellation)?;
            check_assertion(&forward, assertion, index + 1)?;
        }
        phases.push(format!(
            "Requested postcheck assertions: PASS ({} assertion(s))",
            plan.assertions.len()
        ));
    }
    if let Some(rollback) = &plan.rollback {
        let rollback_copy = isolated_copy(&initial, cancellation)?;
        execute_migrations(root, &rollback_copy, &plan.migrations, cancellation)?;
        execute_file(root, &rollback_copy, rollback, cancellation)?;
        check_foreign_keys(&rollback_copy, "rollback")?;
        phases.push(format!("Requested rollback script execution: PASS ({})", rollback.display()));
    }
    Ok(phases)
}

fn isolated_copy(
    initial: &Connection,
    cancellation: &GateCancellation,
) -> Result<Connection, String> {
    cancellation_check(cancellation)?;
    let mut target = Connection::open_in_memory()
        .map_err(|error| format!("open isolated database copy: {error}"))?;
    let backup = Backup::new(initial, &mut target)
        .map_err(|error| format!("copy initial database: {error}"))?;
    loop {
        cancellation_check(cancellation)?;
        match backup.step(128).map_err(|error| format!("copy initial database: {error}"))? {
            StepResult::Done => break,
            StepResult::More => {}
            StepResult::Busy | StepResult::Locked => std::thread::sleep(Duration::from_millis(1)),
            _ => return Err("SQLite backup returned an unknown step result".to_owned()),
        }
    }
    drop(backup);
    target
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| format!("enable foreign keys on isolated copy: {error}"))?;
    Ok(target)
}

fn execute_migrations(
    root: &Path,
    connection: &Connection,
    migrations: &[PathBuf],
    cancellation: &GateCancellation,
) -> Result<(), String> {
    for path in migrations {
        execute_file(root, connection, path, cancellation)?;
    }
    Ok(())
}

fn execute_file(
    root: &Path,
    connection: &Connection,
    path: &Path,
    cancellation: &GateCancellation,
) -> Result<(), String> {
    cancellation_check(cancellation)?;
    let full_path = root.join(path);
    let file =
        fs::File::open(&full_path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut sql = String::new();
    cancellation
        .reader(file)
        .read_to_string(&mut sql)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    cancellation_check(cancellation)?;
    connection.execute_batch(&sql).map_err(|error| format!("execute {}: {error}", path.display()))
}

fn check_foreign_keys(connection: &Connection, phase: &str) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|error| format!("prepare foreign-key check after {phase}: {error}"))?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("run foreign-key check after {phase}: {error}"))?;
    if rows
        .next()
        .map_err(|error| format!("read foreign-key check after {phase}: {error}"))?
        .is_some()
    {
        return Err(format!("foreign-key violation after {phase}"));
    }
    Ok(())
}

fn cancellation_check(cancellation: &GateCancellation) -> Result<(), String> {
    if cancellation.is_cancelled() { Err("run was cancelled".to_owned()) } else { Ok(()) }
}
