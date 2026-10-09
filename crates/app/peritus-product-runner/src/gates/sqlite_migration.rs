//! Disposable execution checks for conventional `SQLite` migration workspaces.

use std::{fs, path::Path};

use peritus_gates::GateExecutionRecord;
use rusqlite::Connection;

const MAX_SQL_BYTES: u64 = 16 * 1024 * 1024;

pub fn run(root: &Path, command: String) -> GateExecutionRecord {
    match verify(root) {
        Ok(phases) => GateExecutionRecord {
            command,
            label: "SQLite migration verification".to_owned(),
            exit_code: Some(0),
            output: format!("{}\nSQLite migration verification: PASS\n", phases.join("\n")),
        },
        Err(detail) => GateExecutionRecord {
            command,
            label: "SQLite migration verification".to_owned(),
            exit_code: Some(1),
            output: format!("SQLite migration verification: FAIL: {detail}\n"),
        },
    }
}

fn verify(root: &Path) -> Result<Vec<String>, String> {
    let schema = root.join("schema.sql");
    let migrations = discover_migrations(root)?;
    let connection =
        Connection::open_in_memory().map_err(|error| format!("open database: {error}"))?;
    let mut phases = Vec::new();

    execute_file(&connection, &schema)?;
    phases.push("Schema execution: PASS".to_owned());

    execute_files(&connection, &migrations)?;
    check_foreign_keys(&connection, "first migration")?;
    phases.push(format!("First migration run: PASS ({} file(s))", migrations.len()));

    execute_files(&connection, &migrations)?;
    check_foreign_keys(&connection, "second migration")?;
    phases.push("Second migration run: PASS".to_owned());

    let postcheck = root.join("postcheck.sql");
    if postcheck.is_file() {
        execute_file(&connection, &postcheck)?;
        phases.push("Postcheck execution: PASS".to_owned());
    }

    let rollback = root.join("rollback.sql");
    if rollback.is_file() {
        execute_file(&connection, &rollback)?;
        check_foreign_keys(&connection, "rollback")?;
        phases.push("Rollback execution: PASS".to_owned());
    }

    Ok(phases)
}

fn discover_migrations(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let single = root.join("migration.sql");
    if !single.is_file() {
        return Err("migration.sql is missing".to_owned());
    }
    Ok(vec![single])
}

fn execute_files(connection: &Connection, paths: &[std::path::PathBuf]) -> Result<(), String> {
    for path in paths {
        execute_file(connection, path)?;
    }
    Ok(())
}

fn execute_file(connection: &Connection, path: &Path) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("inspect {}: {error}", path.display()))?;
    if metadata.len() > MAX_SQL_BYTES {
        return Err(format!("{} exceeds the {MAX_SQL_BYTES}-byte limit", path.display()));
    }
    let sql =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixture(root: &Path, migration: &str) {
        fs::write(
            root.join("schema.sql"),
            "PRAGMA foreign_keys=ON;\nCREATE TABLE users(id INTEGER PRIMARY KEY);\n",
        )
        .expect("schema");
        fs::write(root.join("migration.sql"), migration).expect("migration");
    }

    #[test]
    fn executes_forward_twice_postcheck_and_rollback() {
        let root = tempfile::tempdir().expect("workspace");
        write_fixture(
            root.path(),
            "CREATE TABLE IF NOT EXISTS notes(id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id));\n",
        );
        fs::write(root.path().join("postcheck.sql"), "SELECT COUNT(*) FROM notes;\n")
            .expect("postcheck");
        fs::write(root.path().join("rollback.sql"), "DROP TABLE IF EXISTS notes;\n")
            .expect("rollback");

        let record = run(root.path(), "sqlite-migration".to_owned());

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("Second migration run: PASS"));
        assert!(record.output.contains("Postcheck execution: PASS"));
        assert!(record.output.contains("Rollback execution: PASS"));
        assert!(record.output.contains("Rollback execution: PASS\nSQLite migration verification"));
    }

    #[test]
    fn rejects_a_migration_that_fails_on_the_second_run() {
        let root = tempfile::tempdir().expect("workspace");
        write_fixture(root.path(), "CREATE TABLE notes(id INTEGER PRIMARY KEY);\n");

        let record = run(root.path(), "sqlite-migration".to_owned());

        assert_eq!(record.exit_code, Some(1));
        assert!(record.output.contains("table notes already exists"));
    }
}
