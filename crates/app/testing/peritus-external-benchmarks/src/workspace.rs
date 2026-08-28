//! Exact benchmark workspace validation and baseline preparation.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::BenchmarkError;

const ARTIFACT_MANIFEST: &str = "schema_version = 1\nkind = \"artifact\"\n";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Baseline {
    pub root: PathBuf,
    pub head: String,
    pub initialized_repository: bool,
    pub created_artifact_manifest: bool,
}

pub fn prepare(path: &Path) -> Result<Baseline, BenchmarkError> {
    let root = path
        .canonicalize()
        .map_err(|error| BenchmarkError::filesystem("canonicalize workspace", path, error))?;
    if !root.is_dir() {
        return Err(BenchmarkError::Workspace(format!("{} is not a directory", root.display())));
    }
    let initialized_repository = !root.join(".git").exists();
    let artifact_manifest = root.join("peritus-workspace.toml");
    let created_artifact_manifest = initialized_repository && !artifact_manifest.exists();
    if initialized_repository {
        if created_artifact_manifest {
            std::fs::write(&artifact_manifest, ARTIFACT_MANIFEST).map_err(|error| {
                BenchmarkError::filesystem(
                    "create artifact workspace manifest",
                    &artifact_manifest,
                    error,
                )
            })?;
        }
        checked(&root, "initialize benchmark Git baseline", &["init", "--quiet"])?;
        checked(&root, "stage benchmark fixtures", &["add", "--all", "."])?;
        checked(
            &root,
            "commit benchmark fixtures",
            &[
                "-c",
                "user.name=Peritus Benchmark",
                "-c",
                "user.email=peritus-benchmark@localhost",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "Establish benchmark fixture baseline",
            ],
        )?;
    }
    let head = output(&root, "resolve benchmark Git baseline", &["rev-parse", "--verify", "HEAD"])?;
    Ok(Baseline {
        root,
        head: head.trim().to_owned(),
        initialized_repository,
        created_artifact_manifest,
    })
}

fn checked(root: &Path, operation: &'static str, arguments: &[&str]) -> Result<(), BenchmarkError> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| BenchmarkError::filesystem(operation, root, error))?;
    if !output.status.success() {
        return Err(BenchmarkError::Command {
            operation,
            status: output.status.to_string(),
            detail: bounded(&output.stderr),
        });
    }
    Ok(())
}

fn output(
    root: &Path,
    operation: &'static str,
    arguments: &[&str],
) -> Result<String, BenchmarkError> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| BenchmarkError::filesystem(operation, root, error))?;
    if !output.status.success() {
        return Err(BenchmarkError::Command {
            operation,
            status: output.status.to_string(),
            detail: bounded(&output.stderr),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn bounded(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(64 * 1024)]).into_owned()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn creates_one_committed_fixture_baseline() {
        let root = tempfile::tempdir().expect("temporary workspace");
        fs::write(root.path().join("fixture.txt"), "fixture\n").expect("fixture");

        let first = prepare(root.path()).expect("first preparation");
        let second = prepare(root.path()).expect("second preparation");

        assert!(first.initialized_repository);
        assert!(first.created_artifact_manifest);
        assert!(!second.initialized_repository);
        assert!(!second.created_artifact_manifest);
        assert_eq!(first.head, second.head);
        assert_eq!(fs::read_to_string(root.path().join("fixture.txt")).unwrap(), "fixture\n");
        assert_eq!(
            fs::read_to_string(root.path().join("peritus-workspace.toml")).unwrap(),
            ARTIFACT_MANIFEST,
        );
    }

    #[test]
    fn preserves_existing_repository_head_and_changes() {
        let root = tempfile::tempdir().expect("temporary workspace");
        fs::write(root.path().join("tracked.txt"), "before\n").expect("fixture");
        let first = prepare(root.path()).expect("baseline");
        fs::write(root.path().join("tracked.txt"), "after\n").expect("change");

        let second = prepare(root.path()).expect("existing repository");

        assert!(!second.initialized_repository);
        assert!(!second.created_artifact_manifest);
        assert_eq!(first.head, second.head);
        assert_eq!(fs::read_to_string(root.path().join("tracked.txt")).unwrap(), "after\n");
    }
}
