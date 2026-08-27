//! Shared black-box daemon test configuration.

use std::{fs, path::Path};

use peritus_approval::CredentialRegistrySnapshot;
use peritus_daemon::DaemonConfig;
use peritus_types::RevisionNumber;

/// Builds one strict, isolated daemon configuration beneath `root`.
pub fn configuration(root: &Path) -> DaemonConfig {
    let state = root.join("state");
    let artifacts = state.join("artifacts");
    let evidence = state.join("evidence");
    let workspaces = state.join("workspaces");
    let processes = state.join("processes");
    let transactions = state.join("transactions");
    let backups = state.join("backups");
    let approval_registry = root.join("approval-registry.bin");
    let snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
        .expect("valid public approval registry");
    fs::write(
        &approval_registry,
        snapshot.canonical_bytes().expect("canonical public approval registry"),
    )
    .expect("write public approval registry fixture");
    let text = format!(
        r#"version = 1
store_id = "11111111111111111111111111111111"

[paths]
state_root = {}
artifact_root = {}
evidence_root = {}
workspace_root = {}
process_root = {}
transaction_root = {}
backup_root = {}

[approval_registry]
payload_file = {}
generation = 1

[human]
actor_id = "22222222222222222222222222222222"

[telemetry]
mode = "disabled"
"#,
        toml_path(&state),
        toml_path(&artifacts),
        toml_path(&evidence),
        toml_path(&workspaces),
        toml_path(&processes),
        toml_path(&transactions),
        toml_path(&backups),
        toml_path(&approval_registry),
    );
    DaemonConfig::parse(&text).expect("valid strict daemon configuration")
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}
