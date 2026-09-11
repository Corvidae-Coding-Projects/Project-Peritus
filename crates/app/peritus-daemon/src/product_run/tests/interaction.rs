use super::support::{named_tool_response, text_response};
use super::*;
use peritus_app_protocol::{
    ProductActivityKind, ProductInteractionMode, ProductInteractionRequest, ProductRoleModels,
    ProductRunContinuation, ProductRunConversationQuery,
};

pub(super) fn block_on(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future);
}

#[test]
fn a_question_finishes_without_design_or_workspace_changes_and_restores_durably() {
    block_on(question_scenario());
}
async fn question_scenario() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer =
        scripted(0x31, "chat", vec![text_response(b"Hello. What would you like to discuss?")]);
    let reviewer = scripted(0x32, "review", Vec::new());
    let fixer = scripted(0x33, "fix", Vec::new());
    let workspace_id = WorkspaceId::new([0x34; 16]).expect("workspace");
    let run_id = RunId::new([0x35; 16]).expect("run");
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
        "Hello".to_owned(),
    )
    .expect("request");
    let first = service
        .interact(ProductInteractionRequest::new(
            request,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
        ))
        .await
        .expect("start chat");
    assert_eq!(first.received(), 1);
    let terminal = wait_for_terminal(&service, run_id).await;
    assert_eq!(terminal.phase(), ProductRunPhase::WaitingForUser, "{}", terminal.summary());
    assert!(terminal.status().starts_with("Idle"));
    assert!(terminal.deliverable().is_none());
    assert!(!repository.path().join(".design").exists());
    let snapshot = service
        .query_interaction(ProductRunConversationQuery::new(run_id))
        .expect("chat observation");
    assert_eq!(snapshot.incorporated(), 1);
    {
        let requests = writer.requests.lock().expect("observed provider requests");
        assert!(
            requests[0].messages().iter().flat_map(peritus_model_protocol::Message::content).any(
                |block| {
                    matches!(block, peritus_model_protocol::ContentBlock::Text(text)
                if text.expose_for_wire().contains("Before your first tool call")
                    && text.expose_for_wire().contains("final response must still follow"))
                }
            )
        );
    }
    assert!(
        snapshot
            .activities()
            .iter()
            .any(|activity| activity.kind() == ProductActivityKind::Assistant
                && activity.text().contains("Hello"))
    );
    let response = AppResponseEnvelope::new(
        ProtocolContext::new(
            ProtocolId::new([1; 16]).expect("protocol"),
            ProtocolVersion::new(1, 0).expect("version"),
            SessionId::new([2; 16]).expect("session"),
        ),
        RequestId::new([3; 16]).expect("request"),
        CorrelationId::new([4; 16]).expect("correlation"),
        peritus_app_protocol::AppResponsePayload::Interaction(snapshot.clone()),
    );
    let bytes = encode_app_message(&AppMessage::Response(response), AppProtocolLimits::PRODUCTION)
        .expect("wire-safe chat");
    assert!(!bytes.is_empty());
    let records =
        super::super::persistence::load_records(&service.inner.directory).expect("reload records");
    let restored =
        records.get(&run_id).expect("durable run").interaction.as_ref().expect("durable mode");
    assert_eq!(restored.mode, ProductInteractionMode::Chat);
    assert_eq!(restored.incorporated, 1);
    assert_eq!(restored.activities, snapshot.activities());
    pending_idle_input_is_restarted_without_claiming_prior_incorporation(&service, &writer, run_id)
        .await;
    service.shutdown(Duration::from_secs(5)).await;
}

#[test]
fn public_start_message_is_visible_before_a_stalled_provider_finishes() {
    block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = stalled(0x61, "waiting-writer");
        let reviewer = scripted(0x62, "review", Vec::new());
        let fixer = scripted(0x63, "fix", Vec::new());
        let workspace_id = WorkspaceId::new([0x64; 16]).expect("workspace");
        let run_id = RunId::new([0x65; 16]).expect("run");
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
            "Explain this repository".to_owned(),
        )
        .expect("request");
        service
            .interact(ProductInteractionRequest::new(
                request,
                ProductInteractionMode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = service
                    .query_interaction(ProductRunConversationQuery::new(run_id))
                    .expect("live snapshot");
                if snapshot.incorporated() == 1 {
                    break snapshot;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("public receipt before provider completion");
        assert!(!snapshot.snapshot().phase().terminal());
        assert!(
            snapshot
                .activities()
                .iter()
                .any(|activity| activity.text().starts_with("I'm working on your reply."))
        );
        assert!(
            snapshot
                .activities()
                .iter()
                .all(|activity| activity.kind() != ProductActivityKind::Assistant)
        );
        service
            .control(ProductRunControl::new(run_id, ProductRunControlAction::Cancel))
            .await
            .expect("stop");
        assert_eq!(wait_for_terminal(&service, run_id).await.phase(), ProductRunPhase::Cancelled);
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn follow_up_admitted_at_finalization_is_processed_once() {
    block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x66,
            "chat",
            vec![text_response(b"First reply."), text_response(b"Follow-up considered.")],
        );
        let reviewer = scripted(0x67, "review", Vec::new());
        let fixer = scripted(0x68, "fix", Vec::new());
        let workspace_id = WorkspaceId::new([0x69; 16]).expect("workspace");
        let run_id = RunId::new([0x6a; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let barrier = super::super::execution::inject_finish_barrier(run_id);
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Hello".to_owned(),
        )
        .expect("request");
        service
            .interact(ProductInteractionRequest::new(
                request,
                ProductInteractionMode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start chat");
        tokio::time::timeout(Duration::from_secs(5), barrier.reached())
            .await
            .expect("runner reached finalization barrier");
        let admitted = service
            .continue_run(
                &ProductRunContinuation::new(run_id, "One follow-up".to_owned())
                    .expect("continuation"),
            )
            .await
            .expect("admit follow-up");
        assert!(!admitted.phase().terminal());
        barrier.release();

        let terminal = wait_for_terminal(&service, run_id).await;
        assert_eq!(terminal.phase(), ProductRunPhase::WaitingForUser);
        let conversation = service
            .query_interaction(ProductRunConversationQuery::new(run_id))
            .expect("conversation");
        assert_eq!((conversation.received(), conversation.incorporated()), (2, 2));
        assert_eq!(writer.requests.lock().expect("requests").len(), 2);
        assert_eq!(
            conversation
                .activities()
                .iter()
                .filter(|activity| activity.kind() == ProductActivityKind::User
                    && activity.text() == "One follow-up")
                .count(),
            1
        );
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn interactive_build_narrates_stages_without_changing_terminal_contracts() {
    block_on(pipeline_scenario(ProductInteractionMode::Build));
}

#[test]
fn chat_hands_off_to_the_existing_pipeline_with_the_selected_independent_reviewer() {
    block_on(pipeline_scenario(ProductInteractionMode::Chat));
}

async fn pipeline_scenario(mode: ProductInteractionMode) {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let mut responses = Vec::new();
    if mode == ProductInteractionMode::Chat {
        responses.push(named_tool_response("run_pipeline", b"{}".to_vec()));
    }
    responses.extend(complete_writer(CORRECT));
    let writer = scripted(0x81, "writer", responses);
    let reviewer = scripted(0x82, "reviewer", clean_review());
    let fixer = scripted(0x83, "fixer", Vec::new());
    let workspace_id = WorkspaceId::new([0x84; 16]).expect("workspace");
    let run_id = RunId::new([0x85; 16]).expect("run");
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
    service
        .interact(ProductInteractionRequest::new(request, mode, ProductRoleModels::default()))
        .await
        .expect("start build");
    let terminal = wait_for_terminal(&service, run_id).await;
    assert_eq!(terminal.phase(), ProductRunPhase::Complete, "{}", terminal.summary());
    assert_eq!(reviewer.requests.lock().expect("review requests").len(), 3);
    assert!(fixer.requests.lock().expect("no fixes needed").is_empty());
    let expected_requests = if mode == ProductInteractionMode::Chat { 12 } else { 11 };
    assert_eq!(
        service
            .inner
            .records
            .read()
            .expect("records")
            .get(&run_id)
            .expect("record")
            .progress
            .model_requests,
        expected_requests,
        "Chat handoff and pipeline must share one accounting total"
    );
    assert_eq!(
        terminal.deliverable().expect("qualified candidate").qualification(),
        CandidateStage::Qualified
    );
    let snapshot =
        service.query_interaction(ProductRunConversationQuery::new(run_id)).expect("conversation");
    let public = snapshot
        .activities()
        .iter()
        .map(peritus_app_protocol::ProductActivity::text)
        .collect::<Vec<_>>();
    for message in [
        "I'm moving on to the implementation.",
        "The changes are ready for checks. I'm verifying them now.",
        "The candidate is ready for independent review. I'll check it against your request.",
    ] {
        assert!(public.contains(&message), "missing stage: {message}; {public:?}");
    }
    service.shutdown(Duration::from_secs(5)).await;
}

async fn pending_idle_input_is_restarted_without_claiming_prior_incorporation(
    service: &ProductRunService,
    writer: &Arc<ScriptedProvider>,
    run_id: RunId,
) {
    writer.responses.lock().expect("responses").push_back(text_response(b"New input considered."));
    {
        let mut records = service.inner.records.write().expect("records");
        let record = records.get_mut(&run_id).expect("run");
        record
            .conversation
            .append(
                peritus_app_protocol::ProductConversationRole::User,
                "A correction arriving at the finalization boundary".to_owned(),
            )
            .expect("receive");
        super::super::persist_record(&service.inner.directory, record).expect("durable receipt");
    }
    assert!(service.pending_interactive_input(run_id));
    let before =
        service.query_interaction(ProductRunConversationQuery::new(run_id)).expect("snapshot");
    assert_eq!((before.received(), before.incorporated()), (2, 1));
    service.retry(run_id).await.expect("resume pending input");
    let _ = wait_for_terminal(service, run_id).await;
    let after =
        service.query_interaction(ProductRunConversationQuery::new(run_id)).expect("snapshot");
    assert_eq!((after.received(), after.incorporated()), (2, 2));
    assert!(!service.pending_interactive_input(run_id));
}

#[test]
fn planning_and_review_cannot_execute_a_provider_requested_write() {
    block_on(async {
        for mode in [ProductInteractionMode::Plan, ProductInteractionMode::Review] {
            read_only_write_attempt(mode).await;
        }
    });
}
async fn read_only_write_attempt(mode: ProductInteractionMode) {
    let repository = repository();
    let original = fs::read(repository.path().join("src/lib.rs")).expect("source");
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(
        0x41,
        "plan",
        vec![
            named_tool_response(
                "workspace_write",
                br#"{"path":"src/lib.rs","content":"unauthorized"}"#.to_vec(),
            ),
            text_response(b"I will only discuss the plan."),
        ],
    );
    let reviewer = if mode == ProductInteractionMode::Review {
        Arc::clone(&writer)
    } else {
        scripted(0x42, "review", Vec::new())
    };
    let fixer = scripted(0x43, "fix", Vec::new());
    let workspace_id = WorkspaceId::new([0x44; 16]).expect("workspace");
    let run_id = RunId::new([0x45; 16]).expect("run");
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
        "Discuss a plan".to_owned(),
    )
    .expect("request");
    service
        .interact(ProductInteractionRequest::new(request, mode, ProductRoleModels::default()))
        .await
        .expect("start plan");
    let _ = wait_for_terminal(&service, run_id).await;
    assert_eq!(fs::read(repository.path().join("src/lib.rs")).expect("retained source"), original);
    assert!(!repository.path().join(".design").exists());
    let snapshot =
        service.query_interaction(ProductRunConversationQuery::new(run_id)).expect("activity");
    assert!(
        snapshot.activities().iter().all(|activity| !activity.detail().contains("unauthorized"))
    );
    service.shutdown(Duration::from_secs(5)).await;
}
