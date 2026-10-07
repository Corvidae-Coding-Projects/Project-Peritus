//! Daemon-owned product service construction and durable-generation rehydration.

use super::{
    Arc, BTreeMap, DaemonComponents, DaemonError, Inner, Mutex, Path, PreviewCaptureHost,
    ProcessStore, ProductRunService, RunCancellation, RwLock, WorkspaceCatalog, filesystem, fs, invalid,
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
        request_source_artifacts: peritus_artifact_store::StoreConfig,
    ) -> Result<Self, DaemonError> {
        Self::open_cancellable(
            state_root,
            control_store,
            components,
            workspaces,
            product_policy,
            local_context,
            processes,
            request_source_artifacts,
            &peritus_journal::JournalCancellation::new(),
        )
        .and_then(|service| {
            service.ok_or_else(|| {
                DaemonError::new(
                    crate::DaemonErrorCode::RecoveryRequired,
                    crate::DaemonRecovery::Operator,
                    "open product-run service",
                    "product-run startup stopped without cancellation from its caller",
                )
            })
        })
    }

    pub(crate) fn open_cancellable(
        state_root: &Path,
        control_store: peritus_journal::StoreId,
        components: &DaemonComponents,
        workspaces: &WorkspaceCatalog,
        product_policy: crate::config::ProductRunPolicy,
        local_context: peritus_product_runner::LocalContextConfig,
        processes: ProcessStore,
        request_source_artifacts: peritus_artifact_store::StoreConfig,
        startup_cancellation: &peritus_journal::JournalCancellation,
    ) -> Result<Option<Self>, DaemonError> {
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
        let finding_bodies = persistence::FindingBodyStore::open(
            &control_root.join("finding-bodies"),
        )
        .map_err(|error| {
            DaemonError::with_source(
                crate::DaemonErrorCode::RecoveryRequired,
                crate::DaemonRecovery::Reconcile,
                "open product finding body store",
                "product finding bodies must remain readable before run projections load",
                error,
            )
        })?;
        let (control_generation, mut bootstrap_recovery) =
            crate::product_control::ControlGeneration::open_bootstrapping(
                &control_root,
                control_store,
                startup_cancellation,
            )
        .map_err(|error| {
            DaemonError::with_source(
                crate::DaemonErrorCode::RecoveryRequired,
                crate::DaemonRecovery::Reconcile,
                "open workbench control generation",
                "control state and publication inventory must be owned before run recovery",
                error,
            )
        })?;
        let reply_artifacts = control_generation.reply_artifact_config().clone();
        let control_shutdown = peritus_journal::JournalCancellation::new();
        let control_reconciliation = peritus_journal::JournalCancellation::new();
        let mut loaded_records = None;
        bootstrap_recovery
            .with_control(startup_cancellation, |controls| {
                loaded_records = Some(persistence::load_workbench_records(
                    &control_root,
                    Some(controls),
                    &finding_bodies,
                ));
                Ok(())
            })
            .map_err(|error| {
                DaemonError::with_source(
                    crate::DaemonErrorCode::RecoveryRequired,
                    crate::DaemonRecovery::Reconcile,
                    "recover workbench control generation",
                    "run projections must be checked against their exact control authority",
                    error,
                )
            })?;
        let loaded_records = loaded_records
            .ok_or_else(|| {
                DaemonError::new(
                    crate::DaemonErrorCode::RecoveryRequired,
                    crate::DaemonRecovery::Reconcile,
                    "recover workbench control generation",
                    "bootstrap recovery returned without a run projection inventory",
                )
            })??;
        for (run, record) in loaded_records {
            records.insert(run, record);
        }
        let mut retained_reply_owners = std::collections::BTreeSet::new();
        let mut recovery_cancellations = BTreeMap::new();
        for record in records.values() {
            let (obligation, pending) = match record.settlement_obligation.as_ref() {
                Some(persistence::SettlementObligation::Pending(obligation)) => {
                    (obligation, true)
                }
                Some(persistence::SettlementObligation::Resolved(obligation)) => {
                    (obligation, false)
                }
                Some(
                    persistence::SettlementObligation::LegacyAssessmentRequired(_)
                    | persistence::SettlementObligation::ResolvedWithLegacyGap(_),
                )
                | None => continue,
            };
            let start = &record.interaction.workbench;
            if obligation.start_operation != *start.id().as_bytes()
                || obligation.conversation != *start.conversation().as_bytes()
            {
                return Err(invalid(
                    "terminal obligation does not bind its retained start operation",
                ));
            }
            let reference = obligation.reply_reference().map_err(|error| {
                DaemonError::new(
                    crate::DaemonErrorCode::CorruptState,
                    crate::DaemonRecovery::Operator,
                    "recover terminal public-reply obligation",
                    error.describe(),
                )
            })?;
            if pending {
                let reconciliation = control_generation
                    .register_reconciliation(crate::product_control::AuthoritySet::new([
                        crate::product_control::AuthorityKey::Conversation(start.conversation()),
                        crate::product_control::AuthorityKey::Run(record.request.run_id()),
                    ]))
                    .map_err(|error| {
                        DaemonError::with_source(
                            crate::DaemonErrorCode::RecoveryRequired,
                            crate::DaemonRecovery::Reconcile,
                            "register terminal obligation owner",
                            "the pending terminal obligation must retain its exact C0 authorities",
                            error,
                        )
                    })?;
                recovery_cancellations.insert(
                    record.request.run_id(),
                    RunCancellation {
                        actor: *start.actor_bytes(),
                        continuation: None,
                        attempt_cancelled: Arc::clone(&record.cancelled),
                        control: record.control_cancellation.clone(),
                        provider: record.provider_cancellation.clone(),
                        reconciliation: Some(Arc::new(reconciliation)),
                        user_requested: std::sync::atomic::AtomicBool::new(false),
                        requested_generation: 0,
                        acknowledged_generation: 0,
                        cancellation_retainer: false,
                        owner_dropped: false,
                        active: std::sync::atomic::AtomicBool::new(true),
                        launched: std::sync::atomic::AtomicBool::new(true),
                        ungoverned: matches!(
                            start.intent(),
                            peritus_product_runner::control::ControlIntent::StartExecution { .. }
                        ),
                    },
                );
            }
            let actor = peritus_types::ActorId::new(*start.actor_bytes())
                .map_err(|_| invalid("terminal obligation has an invalid reply actor"))?;
            let workspace = peritus_types::WorkspaceId::new(*start.workspace_bytes())
                .map_err(|_| invalid("terminal obligation has an invalid reply workspace"))?;
            let operation = if obligation.reply_expected_revision == 0 {
                let mut accepted = None;
                bootstrap_recovery
                    .with_control(startup_cancellation, |controls| {
                        accepted = controls
                            .operation(start.conversation(), reference.operation())?;
                        Ok(())
                    })
                    .map_err(|error| {
                        DaemonError::with_source(
                            crate::DaemonErrorCode::CorruptState,
                            crate::DaemonRecovery::Operator,
                            "recover terminal public-reply operation",
                            "the accepted reply operation could not be inspected",
                            error,
                        )
                    })?;
                accepted.ok_or_else(|| {
                    invalid("retained terminal reply has no accepted control operation")
                })?
            } else {
                peritus_product_runner::control::ControlOperation::new(
                    reference.operation(),
                    start.conversation(),
                    actor,
                    workspace,
                    obligation.reply_expected_revision,
                    peritus_product_runner::control::ControlIntent::PublishReply(
                        reference.clone(),
                    ),
                )
            };
            if operation.conversation() != start.conversation()
                || operation.actor_bytes() != start.actor_bytes()
                || operation.workspace_bytes() != start.workspace_bytes()
            {
                return Err(invalid(
                    "terminal reply operation differs from its retained start authority",
                ));
            }
            let claim = crate::product_control::reply_publication_claim(&operation, &reference)
                .map_err(|error| {
                    DaemonError::with_source(
                        crate::DaemonErrorCode::CorruptState,
                        crate::DaemonRecovery::Operator,
                        "recover terminal public-reply obligation",
                        "the retained reply marker claim does not match its obligation",
                        error,
                    )
                })?;
            let retained = bootstrap_recovery
                .inspect_publication(claim.namespace(), claim.id())
                .map_err(|error| {
                    DaemonError::with_source(
                        crate::DaemonErrorCode::CorruptState,
                        crate::DaemonRecovery::Operator,
                        "inspect terminal public-reply marker",
                        "the terminal reply publication inventory is unreadable",
                        error,
                    )
                })?;
            if let Some(retained) = retained {
                if retained.claim() != &claim {
                    return Err(invalid(
                        "terminal public-reply marker differs from its durable obligation",
                    ));
                }
                bootstrap_recovery.claim_publication(&claim).map_err(|error| {
                    DaemonError::with_source(
                        crate::DaemonErrorCode::CorruptState,
                        crate::DaemonRecovery::Operator,
                        "claim terminal public-reply marker",
                        "the durable terminal obligation could not retain its reply artifact",
                        error,
                    )
                })?;
                retained_reply_owners.insert(reference.operation());
            } else if !pending {
                return Err(invalid(
                    "resolved terminal reply has no retained publication marker",
                ));
            }
        }
        bootstrap_recovery.mark_owners_registered().map_err(|error| {
            DaemonError::with_source(
                crate::DaemonErrorCode::RecoveryRequired,
                crate::DaemonRecovery::Reconcile,
                "seal workbench recovery owners",
                "every pending run owner must be registered before foreground admission",
                error,
            )
        })?;
        bootstrap_recovery
            .finish_bootstrap(startup_cancellation)
            .map_err(|error| {
                DaemonError::with_source(
                    crate::DaemonErrorCode::RecoveryRequired,
                    crate::DaemonRecovery::Reconcile,
                    "finish workbench control bootstrap",
                    "marker recovery must be quiescent before foreground admission",
                    error,
                )
            })?;
        let catalog_frontier =
            super::catalog::migrate_sequences(&directory, &mut records).map_err(|error| {
                DaemonError::new(
                    crate::DaemonErrorCode::CorruptState,
                    crate::DaemonRecovery::Operator,
                    "migrate product-run catalog membership",
                    error.describe(),
                )
            })?;
        reconcile_restored_candidates(&directory, &mut records, &workspace_roots)?;
        let improvements = super::improvements::with_startup_cancellation(
            startup_cancellation,
            || super::improvements::Store::open(&state_root.join("improvements.sqlite3")),
        )
        .map_err(|error| {
            DaemonError::new(
                crate::DaemonErrorCode::CorruptState,
                crate::DaemonRecovery::Operator,
                "open improvement inbox",
                error.describe(),
            )
        })?;
        let mut improvements = match improvements {
            super::improvements::StartupContention::Completed(store) => store,
            super::improvements::StartupContention::Cancelled => return Ok(None),
        };
        let reconciliation = super::improvements::with_startup_cancellation(
            startup_cancellation,
            || super::improvements::reconcile_backfill(&mut improvements, &records),
        )
        .map_err(|error| {
            DaemonError::new(
                crate::DaemonErrorCode::CorruptState,
                crate::DaemonRecovery::Operator,
                "reconcile terminal improvement evidence",
                error.describe(),
            )
        })?;
        if matches!(reconciliation, super::improvements::StartupContention::Cancelled) {
            return Ok(None);
        }
        let model_catalogs =
            super::catalog::ModelCatalogs::with_runs(&records, catalog_frontier).map_err(
                |error| {
                    DaemonError::new(
                        crate::DaemonErrorCode::CorruptState,
                        crate::DaemonRecovery::Operator,
                        "rebuild product-run catalog",
                        error.describe(),
                    )
                },
            )?;
        let service = Self {
            inner: Arc::new(Inner {
                improvements: std::sync::Mutex::new(improvements),
                improvement_launch: Mutex::new(()),
                control_generation,
                #[cfg(test)]
                controls: super::workbench::ControlOwnerQueue::new(None),
                control_shutdown,
                control_reconciliation,
                control_store,
                publications: super::publication::RunPublicationManager::new(directory.clone()),
                directory,
                records: RwLock::new(records),
                run_cancellations: std::sync::Mutex::new(recovery_cancellations),
                providers,
                automatic_provider_failover: product_policy.automatic_provider_failover(),
                local_context,
                managed_gate_network: components.managed_gate_network().clone(),
                workspaces: workspace_roots,
                folders: workspaces.folders().clone(),
                processes,
                tasks: Mutex::new(super::ProductRunTasks::new()),
                model_catalogs,
                image_decodes: Arc::new(tokio::sync::Semaphore::new(2)),
                preview_processes: std::sync::Mutex::new(BTreeMap::new()),
                preview_capture: PreviewCaptureHost::discover(),
                #[cfg(test)]
                rewind_faults: std::sync::Mutex::new(Vec::new()),
                host_permissions: permissions::HostPermissionCatalog::new(components, workspaces),
                request_source_artifacts,
                finding_bodies,
                request_source_readers: std::sync::Mutex::new(
                    super::RequestSourceReaders::default(),
                ),
                reply_artifacts,
                reply_readers: std::sync::Mutex::new(
                    super::RequestSourceReaders::default(),
                ),
                retained_reply_owners: std::sync::Mutex::new(retained_reply_owners),
            }),
        };
        service.recover_terminal_obligations().map_err(|error| {
            DaemonError::new(
                crate::DaemonErrorCode::RecoveryRequired,
                crate::DaemonRecovery::Reconcile,
                "recover terminal obligations",
                error.describe(),
            )
        })?;
        Ok(Some(service))
    }
}
