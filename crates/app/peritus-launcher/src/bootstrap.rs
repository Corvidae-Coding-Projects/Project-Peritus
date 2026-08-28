//! Resumable protected local bootstrap.

use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

use peritus_approval::{CredentialRegistrySnapshot, decode_credential_registry};
use peritus_daemon::{DaemonConfig, DaemonIdentity, DaemonPaths, LocalEndpointAddress};
use peritus_product_state::{BootstrapPhase, ProductState};
use peritus_types::RevisionNumber;

use crate::{
    AppLayout, LauncherError,
    persistence::{ProductStateStore, protect_file, publish_new, read_exact_or_publish},
};

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
        let mut state = store.load_or_initialize()?;

        ensure_registry(&self.layout)?;
        if state.bootstrap_phase() == BootstrapPhase::IdentityReady {
            state.advance(BootstrapPhase::RegistryReady)?;
            store.commit(&state)?;
        }

        let configuration = ensure_configuration(&self.layout, &state)?;
        if state.bootstrap_phase() == BootstrapPhase::RegistryReady {
            state.advance(BootstrapPhase::ConfigurationReady)?;
            store.commit(&state)?;
        }

        let endpoint = endpoint(&configuration);
        Ok(PreparedProduct { layout: self.layout, state, configuration, endpoint })
    }
}

/// Fully prepared local product composition ready for daemon startup.
pub struct PreparedProduct {
    layout: AppLayout,
    state: ProductState,
    configuration: DaemonConfig,
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
        self.layout.daemon_config()
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

struct BootstrapLock {
    _file: File,
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
        Ok(Self { _file: file })
    }
}

fn ensure_registry(layout: &AppLayout) -> Result<(), LauncherError> {
    let snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())?;
    let expected = snapshot.canonical_bytes()?;
    let actual = read_exact_or_publish(&layout.approval_registry(), &expected)?;
    let _validated = decode_credential_registry(&actual)?;
    Ok(())
}

fn ensure_configuration(
    layout: &AppLayout,
    state: &ProductState,
) -> Result<DaemonConfig, LauncherError> {
    let text = render_configuration(layout, state)?;
    let expected = DaemonConfig::parse(&text)?;
    let path = layout.daemon_config();
    match DaemonConfig::load(&path) {
        Ok(existing) => {
            validate_configuration_identity(&existing, &expected)?;
            Ok(existing)
        }
        Err(_) if !path.exists() => {
            publish_new(&path.with_extension("pending"), &path, text.as_bytes())?;
            Ok(expected)
        }
        Err(error) => Err(LauncherError::DaemonConfig(error)),
    }
}

fn validate_configuration_identity(
    actual: &DaemonConfig,
    expected: &DaemonConfig,
) -> Result<(), LauncherError> {
    if actual.store_identity()? != expected.store_identity()?
        || actual.human().actor_identity()? != expected.human().actor_identity()?
        || actual.paths() != expected.paths()
        || actual.approval_registry() != expected.approval_registry()
    {
        return Err(LauncherError::PlatformPaths(
            "generated daemon configuration conflicts with this installation identity".to_owned(),
        ));
    }
    Ok(())
}

fn render_configuration(layout: &AppLayout, state: &ProductState) -> Result<String, LauncherError> {
    let daemon_root = layout.state_root().join("daemon");
    let paths = DaemonPaths::new(
        daemon_root.clone(),
        daemon_root.join("artifacts"),
        daemon_root.join("evidence"),
        daemon_root.join("workspaces"),
        daemon_root.join("processes"),
        daemon_root.join("transactions"),
        daemon_root.join("backups"),
    )?;
    Ok(format!(
        "version = 1\nstore_id = {:?}\n\n[paths]\nstate_root = {}\nartifact_root = {}\nevidence_root = {}\nworkspace_root = {}\nprocess_root = {}\ntransaction_root = {}\nbackup_root = {}\n\n[approval_registry]\npayload_file = {}\ngeneration = 1\n\n[human]\nactor_id = {:?}\n\n[telemetry]\nmode = \"disabled\"\n",
        state.identity().store_id(),
        toml_path(paths.state_root())?,
        toml_path(paths.artifact_root())?,
        toml_path(paths.evidence_root())?,
        toml_path(paths.workspace_root())?,
        toml_path(paths.process_root())?,
        toml_path(paths.transaction_root())?,
        toml_path(paths.backup_root())?,
        toml_path(&layout.approval_registry())?,
        state.identity().actor_id(),
    ))
}

fn toml_path(path: &Path) -> Result<String, LauncherError> {
    let text = path.to_str().ok_or_else(|| {
        LauncherError::PlatformPaths(format!(
            "application path is not representable in strict UTF-8 configuration: {}",
            path.display()
        ))
    })?;
    Ok(toml::Value::String(text.to_owned()).to_string())
}

fn endpoint(configuration: &DaemonConfig) -> LocalEndpointAddress {
    let store = configuration.store_identity().expect("validated daemon store identity");
    let identity = DaemonIdentity::new(store);
    #[cfg(unix)]
    {
        LocalEndpointAddress::Unix(
            configuration.paths().state_root().join(format!("{}.sock", identity.endpoint_name())),
        )
    }
    #[cfg(windows)]
    {
        LocalEndpointAddress::Windows(format!(r"\\.\pipe\{}", identity.endpoint_name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_is_complete_and_idempotent() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
        let first = ProductBootstrap::new(layout.clone()).prepare().expect("first bootstrap");
        let identity = first.state().identity().clone();
        assert_eq!(first.state().bootstrap_phase(), BootstrapPhase::ConfigurationReady);
        assert!(first.daemon_config_path().is_file());
        assert!(layout.approval_registry().is_file());

        let second = ProductBootstrap::new(layout).prepare().expect("repeat bootstrap");
        assert_eq!(second.state().identity(), &identity);
        assert_eq!(second.state().generation(), 3);
        assert_eq!(second.endpoint_path(), first.endpoint_path());
    }
}
