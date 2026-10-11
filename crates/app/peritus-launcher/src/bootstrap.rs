//! Resumable protected local bootstrap.

use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

use peritus_approval::{CredentialRegistrySnapshot, decode_credential_registry};
use peritus_daemon::{DaemonConfig, LocalEndpointAddress};
use peritus_product_state::ProviderSelection;
use peritus_product_state::{BootstrapPhase, ProductState, WorkspaceProfile};
use peritus_types::RevisionNumber;

use crate::{
    AppLayout, LauncherError,
    persistence::{ProductStateStore, protect_file, read_exact_or_publish},
};

pub mod configuration;

use configuration::{endpoint, ensure_configuration};

/// Idempotent local product bootstrapper.
pub struct ProductBootstrap {
    layout: AppLayout,
}

impl ProductBootstrap {
    /// Creates a bootstrapper for prepared platform-local roots.
    #[must_use]
    pub const fn new(layout: AppLayout) -> Self {
        Self { layout }
    }

    /// Publishes or validates every durable fact required to start the daemon.
    ///
    /// # Errors
    ///
    /// Returns an exact state, registry, configuration, locking, or filesystem failure.
    pub fn prepare(self) -> Result<PreparedProduct, LauncherError> {
        let lock_path = self.layout.state_root().join("bootstrap.lock");
        let _lock = BootstrapLock::acquire(&lock_path)?;
        let store = ProductStateStore::open(self.layout.product_state_root())?;
        let state = store.load_or_initialize()?;
        finish(self.layout, &store, state)
    }

    /// Persists one canonical provider selection and republishes generated configuration.
    ///
    /// # Errors
    ///
    /// Returns an exact product-state, configuration, locking, or filesystem failure.
    pub fn configure_providers(
        self,
        providers: ProviderSelection,
    ) -> Result<PreparedProduct, LauncherError> {
        if providers
            .enabled()
            .iter()
            .any(|kind| kind.is_account() && providers.account_model(*kind).is_none())
        {
            return Err(LauncherError::Interaction(
                "Select an explicit model for every enabled account provider before saving"
                    .to_owned(),
            ));
        }
        let lock_path = self.layout.state_root().join("bootstrap.lock");
        let _lock = BootstrapLock::acquire(&lock_path)?;
        let store = ProductStateStore::open(self.layout.product_state_root())?;
        let mut state = store.load_or_initialize()?;
        if state.configure_providers(providers) {
            store.commit(&state)?;
        }
        finish(self.layout, &store, state)
    }

    /// Persists one discovered or registered workspace and republishes configuration.
    ///
    /// # Errors
    ///
    /// Returns an exact state, configuration, locking, or filesystem failure.
    pub fn configure_workspace(
        self,
        profile: WorkspaceProfile,
    ) -> Result<PreparedProduct, LauncherError> {
        let lock_path = self.layout.state_root().join("bootstrap.lock");
        let _lock = BootstrapLock::acquire(&lock_path)?;
        let store = ProductStateStore::open(self.layout.product_state_root())?;
        let mut state = store.load_or_initialize()?;
        if state.configure_workspace(profile)? {
            store.commit(&state)?;
        }
        finish(self.layout, &store, state)
    }

    /// Selects one remembered workspace and republishes configuration when changed.
    ///
    /// # Errors
    ///
    /// Returns an unknown-workspace, state, locking, or configuration failure.
    pub fn select_workspace(self, workspace_id: &str) -> Result<PreparedProduct, LauncherError> {
        let lock_path = self.layout.state_root().join("bootstrap.lock");
        let _lock = BootstrapLock::acquire(&lock_path)?;
        let store = ProductStateStore::open(self.layout.product_state_root())?;
        let mut state = store.load_or_initialize()?;
        if state.select_workspace(workspace_id)? {
            store.commit(&state)?;
        }
        finish(self.layout, &store, state)
    }

    /// Forgets one recent workspace and republishes configuration when found.
    ///
    /// # Errors
    ///
    /// Returns an exact state, locking, or configuration failure.
    pub fn remove_workspace(self, workspace_id: &str) -> Result<PreparedProduct, LauncherError> {
        let lock_path = self.layout.state_root().join("bootstrap.lock");
        let _lock = BootstrapLock::acquire(&lock_path)?;
        let store = ProductStateStore::open(self.layout.product_state_root())?;
        let mut state = store.load_or_initialize()?;
        if state.remove_workspace(workspace_id) {
            store.commit(&state)?;
        }
        finish(self.layout, &store, state)
    }
}

/// Fully prepared local product composition ready for daemon startup.
pub struct PreparedProduct {
    layout: AppLayout,
    state: ProductState,
    configuration: DaemonConfig,
    configuration_path: PathBuf,
    endpoint: LocalEndpointAddress,
}

impl PreparedProduct {
    /// Borrows platform-local application paths.
    #[must_use]
    pub const fn layout(&self) -> &AppLayout {
        &self.layout
    }

    /// Borrows durable resumable product state.
    #[must_use]
    pub const fn state(&self) -> &ProductState {
        &self.state
    }

    /// Borrows the validated daemon configuration.
    #[must_use]
    pub const fn daemon_config(&self) -> &DaemonConfig {
        &self.configuration
    }

    /// Returns the generated daemon configuration file.
    #[must_use]
    pub fn daemon_config_path(&self) -> PathBuf {
        self.configuration_path.clone()
    }

    /// Returns the exact local endpoint expected from the stable daemon identity.
    #[must_use]
    pub fn endpoint_path(&self) -> PathBuf {
        match &self.endpoint {
            #[cfg(unix)]
            LocalEndpointAddress::Unix(path) => path.clone(),
            #[cfg(windows)]
            LocalEndpointAddress::Windows(pipe) => PathBuf::from(pipe),
        }
    }
}

fn finish(
    layout: AppLayout,
    store: &ProductStateStore,
    mut state: ProductState,
) -> Result<PreparedProduct, LauncherError> {
    configuration::retain_legacy_models(&layout, store, &mut state)?;
    configuration::account::refresh_executables(&layout, store, &mut state)?;
    ensure_registry(&layout)?;
    if state.bootstrap_phase() == BootstrapPhase::IdentityReady {
        state.advance(BootstrapPhase::RegistryReady)?;
        store.commit(&state)?;
    }
    configuration::folder_catalog::refresh(&layout, store, &mut state)?;
    let (configuration, configuration_path) = ensure_configuration(&layout, &state)?;
    if state.bootstrap_phase() == BootstrapPhase::RegistryReady {
        state.advance(BootstrapPhase::ConfigurationReady)?;
        store.commit(&state)?;
        let configured = ensure_configuration(&layout, &state)?;
        return prepared(layout, state, configured.0, configured.1);
    }
    prepared(layout, state, configuration, configuration_path)
}

fn prepared(
    layout: AppLayout,
    state: ProductState,
    configuration: DaemonConfig,
    configuration_path: PathBuf,
) -> Result<PreparedProduct, LauncherError> {
    let endpoint = endpoint(&configuration)?;
    Ok(PreparedProduct { layout, state, configuration, configuration_path, endpoint })
}

struct BootstrapLock {
    file: File,
}

impl BootstrapLock {
    fn acquire(path: &Path) -> Result<Self, LauncherError> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|error| LauncherError::filesystem("open bootstrap lock", path, error))?;
        protect_file(&file, path)?;
        fs4::FileExt::try_lock(&file).map_err(|_| LauncherError::BootstrapBusy)?;
        Ok(Self { file })
    }
}

impl Drop for BootstrapLock {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

fn ensure_registry(layout: &AppLayout) -> Result<(), LauncherError> {
    let snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())?;
    let expected = snapshot.canonical_bytes()?;
    let actual = read_exact_or_publish(&layout.approval_registry(), &expected)?;
    let _validated = decode_credential_registry(&actual)?;
    Ok(())
}
