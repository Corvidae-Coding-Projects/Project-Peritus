//! Shared black-box daemon test configuration.

use std::path::Path;

use peritus_daemon::DaemonConfig;

/// Builds one strict, isolated daemon configuration beneath `root`.
pub(super) fn configuration(root: &Path) -> DaemonConfig {
    let state = root.join("state");
    let artifacts = state.join("artifacts");
    let evidence = state.join("evidence");
    let workspaces = state.join("workspaces");
    let processes = state.join("processes");
    let transactions = state.join("transactions");
    let backups = state.join("backups");
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
    );
    DaemonConfig::parse(&text).expect("valid strict daemon configuration")
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}
