//! Daemon-owned product service construction and durable-generation rehydration.

use super::{
    Arc, BTreeMap, BTreeSet, DaemonComponents, DaemonError, Inner, Mutex, Path, PreviewCaptureHost,
    ProcessStore, ProductRunService, WorkspaceCatalog, filesystem, fs, invalid, persistence,
};

impl ProductRunService {
    pub(crate) fn open(
        state_root: &Path,
        control_store: peritus_journal::StoreId,
        components: &DaemonComponents,
        workspaces: &WorkspaceCatalog,
        processes: ProcessStore,
        configuration: crate::DaemonConfig,
    ) -> Result<super::OpenedProductRunService, DaemonError> {
        let product_policy = configuration.product();
        let local_context = configuration.context().local().clone();
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
        let indexed = persistence::index::scan(&state_root.join("workbench-v1"));
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
                records: peritus_product_state::publication::PublishedRecords::new(BTreeMap::new()),
                admissions: peritus_product_state::publication::PublicationGates::default(),
                providers,
                automatic_provider_failover: product_policy.automatic_provider_failover(),
                local_context,
                workspaces: super::workspace_catalog::LiveWorkspaces::new(
                    workspaces,
                    components,
                    configuration,
                ),
                processes,
                tasks: Mutex::new(Vec::new()),
                command_recoveries: std::sync::Mutex::new(BTreeSet::new()),
                model_catalogs: super::catalog::ModelCatalogs::default(),
                image_decodes: Arc::new(tokio::sync::Semaphore::new(2)),
                file_reads: Arc::new(tokio::sync::Semaphore::new(2)),
                file_previews: std::sync::Mutex::new(BTreeMap::new()),
                file_refreshes: std::sync::Mutex::new(BTreeMap::new()),
                preview_processes: std::sync::Mutex::new(BTreeMap::new()),
                preview_capture: PreviewCaptureHost::discover(),
            }),
        };
        for (run, hint, path) in indexed {
            let owner = Arc::downgrade(&service.inner);
            let control_root = control_root.clone();
            let candidate_directory = service.inner.directory.clone();
            service
                .inner
                .records
                .insert_deferred(run, hint, move || {
                    let inner = owner.upgrade()?;
                    let controls = match crate::product_control::ControlStore::read_only(
                        &control_root,
                        inner.control_store,
                    ) {
                        Ok(controls) => controls,
                        Err(error) => {
                            crate::diagnostic::report(&format!(
                                "peritusd: deferred run controls unavailable: {error}"
                            ));
                            return None;
                        }
                    };
                    let record = persistence::load_record(&control_root, path.clone(), &controls)?;
                    let mut selected = BTreeMap::from([(run, record)]);
                    let roots = inner.workspaces.roots();
                    if let Err(error) = super::reconcile_restored_candidates(
                        &candidate_directory,
                        &mut selected,
                        &roots,
                    ) {
                        crate::diagnostic::report(&format!(
                            "peritusd: deferred candidate recovery unavailable: {error}"
                        ));
                        return None;
                    }
                    selected.remove(&run)
                })
                .map_err(|_| invalid("run metadata could not be published"))?;
        }
        let live_command_owners = service.reconcile_restored_command_receipts();
        Ok((service, live_command_owners))
    }
}
