//! Daemon-owned product service construction and durable-generation rehydration.

use super::{
    Arc, BTreeMap, BTreeSet, DaemonComponents, DaemonError, Inner, Mutex, Path, PreviewCaptureHost,
    ProcessStore, ProductRunService, RwLock, WorkspaceCatalog, filesystem, fs, invalid,
    permissions, persistence, reconcile_restored_candidates,
};

impl ProductRunService {
    pub(crate) fn open(
        state_root: &Path,
        control_store: peritus_journal::StoreId,
        components: &DaemonComponents,
        workspaces: &WorkspaceCatalog,
        product_policy: crate::config::ProductRunPolicy,
        local_context: peritus_product_runner::LocalContextConfig,
        processes: ProcessStore,
    ) -> Result<super::OpenedProductRunService, DaemonError> {
        let directory = state_root.join("product-runs");
        fs::create_dir_all(&directory).map_err(filesystem)?;
        let mut providers = BTreeMap::new();
        for key in components.providers().keys() {
            if providers.contains_key(&key.profile_id()) {
                return Err(invalid("product provider identity has multiple configured revisions"));
            }
            let provider = components
                .providers()
                .provider(key.profile_id(), key.revision())
                .ok_or_else(|| invalid("configured product provider could not be resolved"))?;
            providers.insert(key.profile_id(), provider);
        }
        let mut records = BTreeMap::new();
        let workspace_roots = workspaces.roots();
        let control_root = state_root.join("workbench-v1");
        let controls = if control_root.join("control.sqlite3").exists() {
            Some(crate::product_control::ControlStore::open(&control_root, control_store).map_err(
                |error| {
                    DaemonError::with_source(
                        crate::DaemonErrorCode::RecoveryRequired,
                        crate::DaemonRecovery::Reconcile,
                        "open workbench control generation",
                        "control state must remain enforced",
                        error,
                    )
                },
            )?)
        } else {
            None
        };
        for (run, record) in persistence::load_workbench_records(&control_root, controls.as_ref())?
        {
            records.insert(run, record);
        }
        reconcile_restored_candidates(&directory, &mut records, &workspace_roots)?;
        let service = Self {
            inner: Arc::new(Inner {
                improvements: std::sync::Mutex::new(
                    super::improvements::Store::open(&state_root.join("improvements.sqlite3"))
                        .map_err(|e| {
                            DaemonError::new(
                                crate::DaemonErrorCode::CorruptState,
                                crate::DaemonRecovery::Operator,
                                "open improvement inbox",
                                e.describe(),
                            )
                        })?,
                ),
                improvement_launch: Mutex::new(()),
                controls: std::sync::Mutex::new(controls),
                control_store,
                directory,
                records: RwLock::new(records),
                providers,
                automatic_provider_failover: product_policy.automatic_provider_failover(),
                local_context,
                workspaces: workspace_roots,
                folders: workspaces.folders().clone(),
                processes,
                tasks: Mutex::new(Vec::new()),
                command_recoveries: std::sync::Mutex::new(BTreeSet::new()),
                model_catalogs: super::catalog::ModelCatalogs::default(),
                image_decodes: Arc::new(tokio::sync::Semaphore::new(2)),
                preview_processes: std::sync::Mutex::new(BTreeMap::new()),
                preview_capture: PreviewCaptureHost::discover(),
                host_permissions: permissions::HostPermissionCatalog::new(components, workspaces),
            }),
        };
        let live_command_owners = service.reconcile_restored_command_receipts();
        Ok((service, live_command_owners))
    }
}
