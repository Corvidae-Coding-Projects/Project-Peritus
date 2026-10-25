//! Live workspace metadata snapshots, independent of provider/runtime ownership and client lifetime.

use super::{
    ProductRunService,
    permissions::{HostPermissionCatalog, HostPermissionEntry},
};
use crate::{
    AuthorityHandle, DaemonComponents, DaemonConfig, DaemonError,
    startup::workspace::WorkspaceCatalog,
};
use peritus_types::WorkspaceId;
use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    sync::RwLock,
};
use tokio::sync::Mutex;

const MAX_CONFIGURATION_BYTES: u64 = 1024 * 1024;

pub(super) struct LiveWorkspaces {
    view: RwLock<View>,
    reload: Mutex<()>,
    network: bool,
}

struct View {
    roots: BTreeMap<WorkspaceId, PathBuf>,
    source_roots: BTreeMap<WorkspaceId, PathBuf>,
    folders: BTreeMap<WorkspaceId, crate::config::FolderDeclaration>,
    permissions: HostPermissionCatalog,
    config: DaemonConfig,
    generation: u64,
    updates_enabled: bool,
}

impl LiveWorkspaces {
    pub(super) fn new(
        catalog: &WorkspaceCatalog,
        components: &DaemonComponents,
        config: DaemonConfig,
    ) -> Self {
        let network = !components.providers().keys().is_empty();
        Self {
            view: RwLock::new(View::new(catalog, network, config, 0)),
            reload: Mutex::new(()),
            network,
        }
    }
    pub(super) fn get(&self, id: &WorkspaceId) -> Option<PathBuf> {
        self.view.read().ok()?.roots.get(id).cloned()
    }
    pub(super) fn roots(&self) -> BTreeMap<WorkspaceId, PathBuf> {
        self.view.read().map(|view| view.roots.clone()).unwrap_or_default()
    }
    pub(super) fn source_root(&self, id: &WorkspaceId) -> Option<PathBuf> {
        self.view.read().ok()?.source_roots.get(id).cloned()
    }
    pub(super) fn contains_key(&self, id: &WorkspaceId) -> bool {
        self.view.read().is_ok_and(|view| view.roots.contains_key(id))
    }
    pub(super) fn folder(&self, id: &WorkspaceId) -> Option<crate::config::FolderDeclaration> {
        self.view.read().ok()?.folders.get(id).cloned()
    }
    pub(super) fn is_direct(&self, id: &WorkspaceId) -> bool {
        self.view.read().is_ok_and(|view| view.folders.contains_key(id))
    }
    pub(super) fn module_jobs(
        &self,
    ) -> Option<peritus_product_runner::NativeModuleJobConfiguration> {
        let view = self.view.read().ok()?;
        let config = view.config.rsi()?;
        Some(peritus_product_runner::NativeModuleJobConfiguration {
            bubblewrap: config.bubblewrap.clone(),
            linux_helper: config.linux_helper.clone(),
            cgroup_root: config.cgroup_root.clone(),
        })
    }
    pub(super) fn permissions(
        &self,
        id: WorkspaceId,
    ) -> Result<HostPermissionEntry, crate::product_control::ControlStoreError> {
        self.view
            .read()
            .map_err(|_| {
                crate::product_control::ControlStoreError::Corrupt(
                    "workspace catalog lock poisoned",
                )
            })?
            .permissions
            .get(id)
    }
}

impl View {
    fn new(
        catalog: &WorkspaceCatalog,
        network: bool,
        config: DaemonConfig,
        generation: u64,
    ) -> Self {
        Self {
            roots: catalog.roots(),
            source_roots: catalog.source_roots(),
            folders: catalog.folders().clone(),
            permissions: HostPermissionCatalog::for_network(network, catalog),
            config,
            generation,
            updates_enabled: false,
        }
    }
}

impl ProductRunService {
    pub(crate) async fn watch_workspace_catalog(
        &self,
        authority: &AuthorityHandle,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) {
        if !self.inner.workspaces.view.read().is_ok_and(|view| view.updates_enabled) {
            return;
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous_failure = None;
        loop {
            tokio::select! {
                biased;
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() { return; }
                }
                _ = interval.tick() => {
                    let result = tokio::select! {
                        biased;
                        _ = stop.changed() => return,
                        result = self.refresh_workspace_catalog(authority) => result,
                    };
                    match result {
                        Ok(()) => previous_failure = None,
                        Err(error) => {
                            let detail = error.to_string();
                            if previous_failure.as_ref() != Some(&detail) {
                                crate::diagnostic::report(&format!("workspace update rejected: {detail}"));
                                previous_failure = Some(detail);
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn set_workspace_generation(&self, generation: u64) -> Result<(), DaemonError> {
        let root = {
            let mut view = self.inner.workspaces.view.write().map_err(|_| unavailable())?;
            view.generation = generation;
            view.updates_enabled = true;
            view.config.paths().state_root().to_owned()
        };
        publish_acknowledgment(&root, generation)
    }

    pub(crate) async fn refresh_workspace_catalog(
        &self,
        authority: &AuthorityHandle,
    ) -> Result<(), DaemonError> {
        let _reload = self.inner.workspaces.reload.lock().await;
        let (root, floor) = {
            let view = self.inner.workspaces.view.read().map_err(|_| unavailable())?;
            (view.config.paths().state_root().to_owned(), view.generation)
        };
        let requested_root = root.clone();
        let candidate =
            tokio::task::spawn_blocking(move || newest_configuration(&requested_root, floor))
                .await
                .map_err(|_| unavailable())??;
        let Some((generation, mut config)) = candidate else {
            // A transient acknowledgment write failure must remain recoverable without
            // reapplying C0 mutations or replacing the daemon.
            return publish_acknowledgment(&root, floor);
        };
        let previous = self.inner.workspaces.view.read().map_err(|_| unavailable())?.config.clone();
        if !previous.accepts_workspace_update(&config) {
            return Err(super::invalid(
                "workspace update changes provider or runtime configuration; the running daemon was preserved",
            ));
        }
        // Recents are a client preference, not revocation of another window's resources.
        // Retain admitted bindings until a planned restart; reject identity replacement.
        config.retain_workspace_bindings(&previous)?;
        let catalog =
            authority.reconcile_workspaces(config.clone(), previous.folders().to_vec()).await?;
        let mut next = View::new(&catalog, self.inner.workspaces.network, config, generation);
        next.updates_enabled = true;
        let root = next.config.paths().state_root().to_owned();
        *self.inner.workspaces.view.write().map_err(|_| unavailable())? = next;
        publish_acknowledgment(&root, generation)
    }
}

fn newest_configuration(
    root: &Path,
    floor: u64,
) -> Result<Option<(u64, DaemonConfig)>, DaemonError> {
    let mut latest = None;
    for entry in fs::read_dir(root).map_err(super::filesystem)? {
        let entry = entry.map_err(super::filesystem)?;
        let name = entry.file_name();
        let Some(generation) = name.to_str().and_then(|name| {
            name.strip_prefix("workspace-update-")?.strip_suffix(".toml")?.parse::<u64>().ok()
        }) else {
            continue;
        };
        if generation > floor && latest.as_ref().is_none_or(|(previous, _)| generation > *previous)
        {
            latest = Some((generation, entry.path()));
        }
    }
    let Some((generation, path)) = latest else { return Ok(None) };
    let metadata = fs::symlink_metadata(&path).map_err(super::filesystem)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIGURATION_BYTES {
        return Err(super::invalid(
            "workspace update must be a bounded regular configuration file",
        ));
    }
    let mut text = String::new();
    fs::File::open(&path)
        .map_err(super::filesystem)?
        .take(MAX_CONFIGURATION_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(super::filesystem)?;
    if text.len() as u64 > MAX_CONFIGURATION_BYTES {
        return Err(super::invalid("workspace update exceeds its bound"));
    }
    let config = DaemonConfig::parse(&text)?;
    if config.paths().state_root() != root {
        return Err(super::invalid("workspace update belongs to a different daemon"));
    }
    Ok(Some((generation, config)))
}

fn publish_acknowledgment(root: &Path, generation: u64) -> Result<(), DaemonError> {
    use std::io::Write as _;
    let expected = format!("generation={generation}\npid={}\n", std::process::id());
    let acknowledgment = root.join("workspace-applied-generation");
    if fs::read_to_string(&acknowledgment).is_ok_and(|actual| actual == expected) {
        return Ok(());
    }
    let pending = root.join("workspace-applied.pending");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&pending)
        .map_err(super::filesystem)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(fs::Permissions::from_mode(0o600)).map_err(super::filesystem)?;
    }
    file.write_all(expected.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(super::filesystem)?;
    drop(file);
    fs::rename(&pending, root.join("workspace-applied-generation")).map_err(super::filesystem)
}

fn unavailable() -> DaemonError {
    super::invalid("live workspace catalog is unavailable")
}
