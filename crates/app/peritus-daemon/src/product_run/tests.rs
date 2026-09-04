//! End-to-end daemon ownership of candidate continuation and user acceptance.

use std::{collections::BTreeMap, fs, sync::Arc, time::Duration};

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppResponseEnvelope, CorrelationId, ProductProviderSelection,
    ProductRunContinuation, ProductRunControl, ProductRunControlAction, ProductRunPhase,
    ProductRunQuery, ProductRunRequest, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    encode_app_message,
};
use peritus_process::ProcessStore;
use peritus_provider_core::ModelProvider;
use peritus_run_settlement::CandidateStage;
use peritus_types::SessionId;
use peritus_types::{RunId, WorkspaceId};

use super::{Inner, ProductRunService};

mod support;

use support::{
    CORRECT, ScriptedProvider, clean_review, complete_writer, repository, scripted, stalled,
};

#[test]
fn candidate_can_continue_through_the_daemon_and_be_accepted() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(candidate_continuation_scenario());
}

async fn candidate_continuation_scenario() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x71, "writer", complete_writer(CORRECT));
    let reviewer = scripted(0x72, "reviewer", Vec::new());
    let fixer = scripted(0x73, "fixer", Vec::new());
    let run_id = RunId::new([0x74; 16]).expect("run");
    let workspace_id = WorkspaceId::new([0x75; 16]).expect("workspace");
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
    let interrupted_deliverable = interrupted.deliverable().expect("candidate deliverable");
    assert_eq!(interrupted.phase(), ProductRunPhase::Failed);
    assert_eq!(interrupted_deliverable.qualification(), CandidateStage::ReviewPending);
    assert!(!interrupted_deliverable.accepted());

    writer.responses.lock().expect("writer scripts").extend(complete_writer(CORRECT));
    reviewer.responses.lock().expect("reviewer scripts").extend(clean_review());
    service
        .continue_run(
            &ProductRunContinuation::new(run_id, "Continue and complete the review.".to_owned())
                .expect("continuation"),
        )
        .await
        .expect("continue candidate");

    let completed = wait_for_terminal(&service, run_id).await;
    assert_eq!(completed.phase(), ProductRunPhase::Complete);
    assert_eq!(
        completed.deliverable().expect("qualified deliverable").qualification(),
        CandidateStage::Qualified,
    );

    let accepted = service
        .control(ProductRunControl::new(run_id, ProductRunControlAction::Accept))
        .await
        .expect("accept deliverable");
    assert!(accepted.deliverable().expect("accepted deliverable").accepted());
    service.shutdown(Duration::from_secs(5)).await;
}

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
fn changed_requirements_continuation_has_a_wire_safe_response_and_retains_the_candidate() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(changed_requirements_continuation_scenario());
}

async fn changed_requirements_continuation_scenario() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x91, "writer", complete_writer(CORRECT));
    let reviewer = scripted(0x92, "reviewer", Vec::new());
    let fixer = scripted(0x93, "fixer", Vec::new());
    let run_id = RunId::new([0x94; 16]).expect("run");
    let workspace_id = WorkspaceId::new([0x95; 16]).expect("workspace");
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

    let queued = service
        .continue_run(
            &ProductRunContinuation::new(run_id, "Change the requested behavior.".to_owned())
                .expect("continuation"),
        )
        .await
        .expect("queue changed requirements");
    let payload = service.project(queued).expect("project continuation response");
    let context = ProtocolContext::new(
        ProtocolId::new([0x96; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([0x97; 16]).expect("session"),
    );
    let response = AppResponseEnvelope::new(
        context,
        RequestId::new([0x98; 16]).expect("request"),
        CorrelationId::new([0x99; 16]).expect("correlation"),
        payload,
    );
    encode_app_message(&AppMessage::Response(response), AppProtocolLimits::PRODUCTION)
        .expect("continuation response must satisfy the public wire contract");

    let failed = wait_for_terminal(&service, run_id).await;
    assert_eq!(failed.phase(), ProductRunPhase::Failed);
    assert_eq!(
        failed.deliverable().expect("retained candidate").qualification(),
        CandidateStage::Observed,
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
    wait_for_phase(&service, run_id, ProductRunPhase::Reviewing).await;
    service.cancel(run_id).expect("cancel run");
    let cancelled = wait_for_terminal(&service, run_id).await;

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
    ProductRunService {
        inner: Arc::new(Inner {
            directory,
            records: std::sync::RwLock::new(BTreeMap::new()),
            providers: registry,
            automatic_provider_failover: false,
            workspaces: BTreeMap::from([(workspace_id, workspace.to_owned())]),
            processes,
            tasks: tokio::sync::Mutex::new(Vec::new()),
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

async fn wait_for_phase(service: &ProductRunService, run_id: RunId, phase: ProductRunPhase) {
    for _ in 0..400 {
        let snapshot = service
            .query(ProductRunQuery::exact(run_id))
            .expect("query run")
            .into_iter()
            .next()
            .expect("run snapshot");
        if snapshot.phase() == phase {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("product run did not reach {phase:?} within ten seconds")
}
