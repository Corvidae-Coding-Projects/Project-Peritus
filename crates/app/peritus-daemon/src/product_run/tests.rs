//! End-to-end daemon ownership of candidate continuation and user acceptance.

use std::{collections::BTreeMap, fs, sync::Arc, time::Duration};

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppResponseEnvelope, CorrelationId, ProductProviderSelection,
    ProductRunControl, ProductRunControlAction, ProductRunPhase, ProductRunQuery, ProtocolContext,
    ProtocolId, ProtocolVersion, RequestId, encode_app_message,
};
use peritus_process::ProcessStore;
use peritus_provider_core::ModelProvider;
use peritus_run_settlement::CandidateStage;
use peritus_types::SessionId;
use peritus_types::{RunId, WorkspaceId};

use super::{Inner, ProductRunRequest, ProductRunService};

mod catalog;
mod discard_recovery;
mod doctor;
mod folder;
mod healing;
mod improvements;
mod interaction;
mod model_selection;
mod observations;
mod recovery;
mod support;
mod task_baseline;
mod workbench;

use support::{
    CORRECT, ScriptedProvider, clean_review, complete_writer, repository, scripted, stalled,
};

#[test]
fn candidate_retry_resumes_review_without_repeating_design_or_writing() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(candidate_retry_scenario());
}

async fn candidate_retry_scenario() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0xa1, "writer", complete_writer(CORRECT));
    let reviewer = scripted(0xa2, "reviewer", Vec::new());
    let fixer = scripted(0xa3, "fixer", Vec::new());
    let run_id = RunId::new([0xa4; 16]).expect("run");
    let workspace_id = WorkspaceId::new([0xa5; 16]).expect("workspace");
    let service =
        service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
    let request = ProductRunRequest::new(
        run_id,
        workspace_id,
        ProductProviderSelection::new(
            writer.profile.profile_id(),
            reviewer.profile.profile_id(),
            fixer.profile.profile_id(),
        ),
        "Add a tested answer function that returns 42.".to_owned(),
    )
    .expect("request");

    service.start(request).await.expect("start run");
    let interrupted = wait_for_terminal(&service, run_id).await;
    assert_eq!(interrupted.phase(), ProductRunPhase::Failed);
    assert_eq!(
        interrupted.deliverable().expect("candidate deliverable").qualification(),
        CandidateStage::ReviewPending,
    );
    assert!(writer.responses.lock().expect("writer scripts").is_empty());

    reviewer.responses.lock().expect("reviewer scripts").extend(clean_review());
    service
        .control(ProductRunControl::new(run_id, ProductRunControlAction::Retry))
        .await
        .expect("retry candidate");

    let completed = wait_for_terminal(&service, run_id).await;
    assert_eq!(completed.phase(), ProductRunPhase::Complete);
    assert_eq!(
        completed.deliverable().expect("qualified deliverable").qualification(),
        CandidateStage::Qualified,
    );
    assert!(
        writer.responses.lock().expect("writer scripts").is_empty(),
        "phase-preserving retry must not invoke the writer again",
    );
    service.shutdown(Duration::from_secs(5)).await;
}

#[test]
fn cancellation_during_review_keeps_the_candidate_actionable() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(candidate_cancellation_scenario());
}

async fn candidate_cancellation_scenario() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x81, "writer", complete_writer(CORRECT));
    let reviewer = stalled(0x82, "reviewer");
    let fixer = scripted(0x83, "fixer", Vec::new());
    let run_id = RunId::new([0x84; 16]).expect("run");
    let workspace_id = WorkspaceId::new([0x85; 16]).expect("workspace");
    let service =
        service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
    let request = ProductRunRequest::new(
        run_id,
        workspace_id,
        ProductProviderSelection::new(
            writer.profile.profile_id(),
            reviewer.profile.profile_id(),
            fixer.profile.profile_id(),
        ),
        "Add a tested answer function that returns 42.".to_owned(),
    )
    .expect("request");

    service.start(request).await.expect("start run");
    wait_for_review_stall(&service, run_id, &reviewer).await;
    service.cancel(run_id).expect("cancel run");
    let cancelled =
        tokio::time::timeout(Duration::from_secs(5), wait_for_terminal(&service, run_id))
            .await
            .expect("run did not settle after reviewer cancellation");

    assert_eq!(cancelled.phase(), ProductRunPhase::Cancelled);
    assert_eq!(
        cancelled.deliverable().expect("candidate deliverable").qualification(),
        CandidateStage::ReviewPending,
    );
    service.shutdown(Duration::from_secs(5)).await;
}

fn service(
    state: &std::path::Path,
    workspace: &std::path::Path,
    workspace_id: WorkspaceId,
    providers: [&Arc<ScriptedProvider>; 3],
) -> ProductRunService {
    let directory = state.join("product-runs");
    fs::create_dir_all(&directory).expect("product run directory");
    let mut registry = BTreeMap::new();
    for provider in providers {
        let profile_id = provider.profile.profile_id();
        let provider: Arc<dyn ModelProvider> = provider.clone();
        registry.insert(profile_id, provider);
    }
    let processes = ProcessStore::open(state.join("processes"), workspace).expect("process store");
    let network = !registry.is_empty();
    ProductRunService {
        inner: Arc::new(Inner {
            improvements: std::sync::Mutex::new(
                super::improvements::Store::open(&state.join("improvements.sqlite3"))
                    .expect("inbox"),
            ),
            improvement_launch: tokio::sync::Mutex::new(()),
            controls: std::sync::Mutex::new(None),
            control_store: peritus_journal::StoreId::new([0x7f; 16]).expect("control store"),
            directory,
            records: std::sync::RwLock::new(BTreeMap::new()),
            providers: registry,
            automatic_provider_failover: false,
            local_context: peritus_product_runner::LocalContextConfig::default(),
            workspaces: BTreeMap::from([(workspace_id, workspace.to_owned())]),
            folders: BTreeMap::new(),
            processes,
            tasks: tokio::sync::Mutex::new(Vec::new()),
            command_recoveries: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            model_catalogs: super::catalog::ModelCatalogs::default(),
            image_decodes: Arc::new(tokio::sync::Semaphore::new(2)),
            preview_processes: std::sync::Mutex::new(BTreeMap::new()),
            preview_capture: super::PreviewCaptureHost::discover(),
            host_permissions: super::permissions::HostPermissionCatalog::managed(
                [workspace_id],
                network,
            ),
        }),
    }
}

async fn wait_for_terminal(
    service: &ProductRunService,
    run_id: RunId,
) -> peritus_app_protocol::ProductRunSnapshot {
    for _ in 0..400 {
        let snapshot = service
            .query(ProductRunQuery::exact(run_id))
            .expect("query run")
            .into_iter()
            .next()
            .expect("run snapshot");
        if snapshot.phase().terminal() {
            return snapshot;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("product run did not settle within ten seconds")
}

async fn wait_for_review_stall(
    service: &ProductRunService,
    run_id: RunId,
    reviewer: &ScriptedProvider,
) {
    tokio::time::timeout(Duration::from_secs(30), reviewer.wait_for_stalled_response())
        .await
        .expect("reviewer did not start its deliberately stalled response");
    let phase = service.query(ProductRunQuery::exact(run_id)).expect("review snapshot")[0].phase();
    assert_eq!(phase, ProductRunPhase::Reviewing, "reviewer stalled in {phase:?}");
}
