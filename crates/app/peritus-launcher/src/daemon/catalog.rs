//! Serialized shared-daemon launch and immutable live workspace configuration publication.

use super::{SiblingBinaries, applied_identity, record_applied_configuration};
use crate::{LauncherError, PreparedProduct};
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    time::{Duration, Instant},
};

pub(super) async fn launch_lock(
    product: &PreparedProduct,
    timeout: Duration,
) -> Result<File, LauncherError> {
    let root = product.daemon_config().paths().state_root();
    fs::create_dir_all(root)
        .map_err(|error| LauncherError::filesystem("prepare launch lock", root, error))?;
    lifecycle_lock(root, timeout).await
}

pub(super) async fn lifecycle_lock(root: &Path, timeout: Duration) -> Result<File, LauncherError> {
    let path = root.join("launch.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| LauncherError::filesystem("open launch lock", &path, error))?;
    crate::persistence::protect_file(&file, &path)?;
    let started = Instant::now();
    loop {
        match fs4::FileExt::try_lock(&file) {
            Ok(()) => return Ok(file),
            Err(fs4::TryLockError::WouldBlock) if started.elapsed() < timeout => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(error) => {
                return Err(LauncherError::DaemonSpawn(format!(
                    "wait for another launcher or shutdown: {error}"
                )));
            }
        }
    }
}

pub(super) async fn update_running(
    product: &PreparedProduct,
    binaries: &SiblingBinaries,
    daemon_digest: &str,
) -> Result<bool, LauncherError> {
    let marker = product.layout().daemon_applied_configuration();
    let Ok(applied) = fs::read_to_string(&marker) else { return Ok(false) };
    let Some(configuration) = applied.lines().find_map(|line| line.strip_prefix("configuration="))
    else {
        return Ok(false);
    };
    if applied != applied_identity(Path::new(configuration), daemon_digest) {
        return Ok(false);
    }
    let current = peritus_daemon::DaemonConfig::load(configuration)?;
    if !current.accepts_workspace_update(product.daemon_config()) {
        return Ok(false);
    }
    let root = product.daemon_config().paths().state_root();
    let generation = product.state().generation();
    let path = root.join(format!("workspace-update-{generation:020}.toml"));
    let bytes = fs::read(product.daemon_config_path())
        .map_err(|error| LauncherError::filesystem("read workspace configuration", &path, error))?;
    let actual = crate::persistence::read_exact_or_publish(&path, &bytes)?;
    if actual != bytes {
        return Err(LauncherError::DaemonSpawn(
            "workspace configuration generation has different content".into(),
        ));
    }
    let result = tokio::time::timeout(
        Duration::from_secs(8),
        tokio::process::Command::new(binaries.application())
            .arg("--endpoint")
            .arg(product.endpoint_path())
            .arg("--json")
            .arg("status")
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let ok = result.is_ok_and(|result| result.is_ok_and(|output| output.status.success()));
    if !ok {
        return Err(LauncherError::DaemonSpawn("The running daemon could not apply the new workspace catalog. Existing sessions were preserved; inspect daemon diagnostics and retry.".into()));
    }
    let started = Instant::now();
    let acknowledged = loop {
        let acknowledgment = fs::read_to_string(root.join("workspace-applied-generation")).ok();
        let instance = fs::read_to_string(root.join("daemon.instance")).ok();
        let acknowledged = acknowledgment.as_deref().and_then(|text| {
            let pid = text.lines().find(|line| line.starts_with("pid="))?;
            if !instance.as_deref()?.lines().any(|line| line == pid) {
                return None;
            }
            text.lines().find_map(|line| line.strip_prefix("generation="))?.parse::<u64>().ok()
        });
        let Some(applied) = acknowledged else {
            return Err(LauncherError::DaemonSpawn("The running daemon does not support live workspace updates. Existing sessions were preserved. Update it during a planned graceful restart, then reopen this workspace.".into()));
        };
        if applied >= generation {
            break acknowledged;
        }
        if started.elapsed() >= Duration::from_secs(8) {
            return Err(LauncherError::DaemonSpawn("The daemon could not finish applying or acknowledging the workspace catalog. Existing sessions were preserved; inspect daemon diagnostics, correct any storage or configuration error, then retry.".into()));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    if acknowledged == Some(generation) {
        record_applied_configuration(product, daemon_digest)?;
    }
    Ok(true)
}
