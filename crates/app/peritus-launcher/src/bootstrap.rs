//! Resumable protected local bootstrap.

use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

use peritus_approval::{CredentialRegistrySnapshot, decode_credential_registry};
use peritus_daemon::{DaemonConfig, LocalEndpointAddress};
use peritus_product_state::ProviderSelection;
use peritus_product_state::{BootstrapPhase, ProductState, WorkspaceProfile};
use peritus_tui::ProductLaunchContext;
use peritus_types::RevisionNumber;

use crate::{
    AppLayout, LauncherError,
    persistence::{ProductStateStore, protect_file, read_exact_or_publish},
};

pub mod configuration;
#[cfg(all(test, unix))]
mod endpoint_tests;
#[cfg(test)]
mod model_tests;

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
            || !providers.model_facts_are_complete()
        {
            return Err(LauncherError::Interaction(
                "Resolve the exact model, capacity, and feature facts for every enabled provider before saving"
                    .to_owned(),
            ));
        }
        let lock_path = self.layout.state_root().join("bootstrap.lock");
        let _lock = BootstrapLock::acquire(&lock_path)?;
        let store = ProductStateStore::open(self.layout.product_state_root())?;
        let mut state = store.load_or_initialize()?;
        if state.configure_providers(providers)? {
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
        if state.remove_workspace(workspace_id)? {
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

    /// Refreshes the immutable product generation while retaining the exact presentation scope.
    ///
    /// # Errors
    ///
    /// Rejects a current generation that changes the selected workspace, route identities,
    /// provider models, default, trust, or explicit failover choice beneath an open interface.
    pub(crate) fn refresh_for_reconciliation(
        &mut self,
        launch: &ProductLaunchContext,
    ) -> Result<(), LauncherError> {
        let refreshed = ProductBootstrap::new(self.layout.clone()).prepare()?;
        ensure_reconciliation_compatible(self, &refreshed, launch)?;
        *self = refreshed;
        Ok(())
    }
}

fn ensure_reconciliation_compatible(
    previous: &PreparedProduct,
    refreshed: &PreparedProduct,
    launch: &ProductLaunchContext,
) -> Result<(), LauncherError> {
    let workspace_id = hex_identifier(launch.workspace_id().as_bytes());
    let previous_workspace = previous.state.workspaces().find(&workspace_id);
    let refreshed_workspace = refreshed.state.workspaces().find(&workspace_id);
    let providers_match = provider_scope_matches(
        previous.state.providers(),
        refreshed.state.providers(),
    );
    let active_workspace_matches = previous.state.workspaces().active()
        == refreshed.state.workspaces().active();
    let target_matches = previous_workspace.is_some()
        && previous_workspace == refreshed_workspace
        && refreshed_workspace.is_some_and(|workspace| {
            workspace
                .managed_root()
                .unwrap_or_else(|| workspace.repository_root())
                == launch.workspace_label()
                && if workspace.is_direct_folder() {
                    launch.direct_folder_writable()
                        == Some(
                            workspace.trust_level()
                                == peritus_product_state::WorkspaceTrust::Trusted,
                        )
                } else {
                    launch.direct_folder_writable().is_none()
                }
        });
    let routes = refreshed.state.providers().routes();
    let launch_routes_match = routes.len() == launch.providers().len()
        && routes.iter().zip(launch.providers()).all(|(route, displayed)| {
            route.identity().as_bytes() == displayed.profile_id().as_bytes()
        });
    let default_matches = refreshed
        .state
        .providers()
        .default_route()
        .and_then(|selected| routes.iter().position(|route| route.identity() == selected))
        == launch.default_provider();
    if previous.state.identity() != refreshed.state.identity()
        || previous.endpoint != refreshed.endpoint
        || refreshed.state.generation() < previous.state.generation()
        || !providers_match
        || !active_workspace_matches
        || !target_matches
        || !launch_routes_match
        || !default_matches
    {
        return Err(LauncherError::Interaction(
            "The durable workspace or provider scope changed while this interface was open. Reopen Peritus to adopt it without retargeting the retained conversation or draft."
                .to_owned(),
        ));
    }
    Ok(())
}

fn provider_scope_matches(
    previous: &ProviderSelection,
    refreshed: &ProviderSelection,
) -> bool {
    previous.enabled() == refreshed.enabled()
        && previous.routes() == refreshed.routes()
        && previous.default_route() == refreshed.default_route()
        && previous.automatic_failover() == refreshed.automatic_failover()
        && previous.direct_profiles() == refreshed.direct_profiles()
        && previous.enabled().iter().copied().filter(|kind| kind.is_account()).all(|kind| {
            previous.account_model(kind) == refreshed.account_model(kind)
                && previous.account_model_facts(kind) == refreshed.account_model_facts(kind)
        })
}

fn hex_identifier(bytes: &[u8; 16]) -> String {
    let mut value = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(value, "{byte:02x}").expect("writing to String cannot fail");
    }
    value
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

    #[test]
    fn provider_selection_publishes_a_new_immutable_configuration() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
        let first = ProductBootstrap::new(layout.clone()).prepare().expect("bootstrap");
        let selection = ProviderSelection::new(
            vec![peritus_product_state::ProviderKind::CodexAccount],
            Some(peritus_product_state::ProviderKind::CodexAccount),
        )
        .expect("selection")
        .with_account_models(std::collections::BTreeMap::from([(
            peritus_product_state::ProviderKind::CodexAccount,
            "fixture-codex-model".to_owned(),
        )]))
        .expect("model selection");
        let configured =
            ProductBootstrap::new(layout).configure_providers(selection).expect("configure");
        assert_ne!(configured.daemon_config_path(), first.daemon_config_path());
        assert_eq!(configured.daemon_config().providers().len(), 1);
        assert!(!configured.daemon_config().product().automatic_provider_failover());
        assert!(first.daemon_config_path().is_file());
    }

    #[test]
    fn explicit_failover_choice_reaches_the_immutable_daemon_configuration() {
        use peritus_product_state::ProviderKind;

        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
        let selection = ProviderSelection::with_direct_profiles_and_failover(
            vec![ProviderKind::CodexAccount, ProviderKind::ClaudeAccount],
            Some(ProviderKind::CodexAccount),
            Vec::new(),
            true,
        )
        .expect("explicit failover selection")
        .with_account_models(std::collections::BTreeMap::from([
            (ProviderKind::CodexAccount, "fixture-codex-model".to_owned()),
            (ProviderKind::ClaudeAccount, "fixture-claude-model".to_owned()),
        ]))
        .expect("model selection");
        let configured =
            ProductBootstrap::new(layout).configure_providers(selection).expect("configure");

        assert!(configured.daemon_config().product().automatic_provider_failover());
        let text = std::fs::read_to_string(configured.daemon_config_path()).expect("configuration");
        assert!(text.contains("[product]\nautomatic_provider_failover = true"));
    }

    #[test]
    fn every_direct_provider_profile_builds_its_production_adapter() {
        use peritus_product_state::{CompatibleProtocol, DirectProviderProfile, ProviderKind};

        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
        let reference = format!("peritus-secret-v1:{}:{}", "01".repeat(16), "02".repeat(32));
        let profiles = vec![
            direct_profile(ProviderKind::OpenAiApi, &reference, None, None),
            direct_profile(
                ProviderKind::AnthropicApi,
                &reference,
                Some("https://api.anthropic.com"),
                None,
            ),
            direct_profile(
                ProviderKind::GoogleGeminiApi,
                &reference,
                Some("https://generativelanguage.googleapis.com"),
                None,
            ),
            DirectProviderProfile::new(
                ProviderKind::CompatibleEndpoint,
                reference,
                Some("https://example.com/v1/responses".to_owned()),
                "compatible-model".to_owned(),
                Some(CompatibleProtocol::Responses),
                None,
            )
            .expect("compatible profile"),
        ];
        let enabled = profiles.iter().map(DirectProviderProfile::kind).collect::<Vec<_>>();
        let selection = ProviderSelection::with_direct_profiles(
            enabled,
            Some(ProviderKind::OpenAiApi),
            profiles,
        )
        .expect("selection");
        let configured =
            ProductBootstrap::new(layout).configure_providers(selection).expect("configure");
        assert_eq!(configured.daemon_config().providers().len(), 4);
        for route in configured.daemon_config().providers() {
            route.declaration().expect("production adapter declaration");
        }
    }

    fn direct_profile(
        kind: peritus_product_state::ProviderKind,
        reference: &str,
        endpoint: Option<&str>,
        protocol: Option<peritus_product_state::CompatibleProtocol>,
    ) -> peritus_product_state::DirectProviderProfile {
        peritus_product_state::DirectProviderProfile::new(
            kind,
            reference.to_owned(),
            endpoint.map(str::to_owned),
            "provider-model".to_owned(),
            protocol,
            None,
        )
        .expect("direct profile")
    }
}
