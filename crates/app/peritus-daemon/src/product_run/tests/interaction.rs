use super::support::{named_tool_response, text_response};
use super::*;
use peritus_app_protocol::{
    ProductActivityKind, ProductInteractionMode, ProductInteractionQuery, ProductRoleModels,
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
        .start_interaction(request, ProductInteractionMode::Chat, ProductRoleModels::default())
        .await
        .expect("start chat");
    assert_eq!(first.received(), 1);
    let terminal = wait_for_terminal(&service, run_id).await;
    assert_eq!(terminal.phase(), ProductRunPhase::WaitingForUser, "{}", terminal.summary());
    assert!(terminal.status().starts_with("Idle"));
    assert!(terminal.deliverable().is_none());
    assert!(!repository.path().join(".design").exists());
    let snapshot =
        service.query_interaction(ProductInteractionQuery::new(run_id)).expect("chat observation");
    assert_eq!(snapshot.incorporated(), 1);
    let actor = {
        let records = service.inner.records.read().unwrap();
        let bytes = *records.get(&run_id).unwrap().interaction.workbench.actor_bytes();
        peritus_types::ActorId::new(bytes).unwrap()
    };
    let binding =
        service.query_interaction_binding(actor, ProductInteractionQuery::new(run_id)).unwrap();
    assert_eq!(
        binding.conversation(),
        peritus_app_protocol::WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new(
                *service
                    .inner
                    .records
                    .read()
                    .unwrap()
                    .get(&run_id)
                    .unwrap()
                    .interaction
                    .workbench
                    .conversation()
                    .as_bytes(),
            )
            .unwrap(),
            workspace_id,
        )
    );
    assert_eq!(binding.interaction(), &snapshot);
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
    let records = service.load_test_records().expect("reload records");
    let restored = &records.get(&run_id).expect("durable run").interaction;
    assert_eq!(restored.mode, ProductInteractionMode::Chat);
    assert_eq!(restored.incorporated, 1);
    assert_eq!(restored.activities, snapshot.activities());
    service.shutdown().await.expect("shutdown product runs");
}

#[test]
fn idle_chat_releases_workspace_ownership_for_a_new_conversation() {
    block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x51,
            "chat",
            vec![text_response(b"First reply."), text_response(b"Second reply.")],
        );
        let reviewer = scripted(0x52, "review", Vec::new());
        let fixer = scripted(0x53, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x54; 16]).expect("workspace");
        let providers = ProductProviderSelection::new(
            writer.profile.profile_id(),
            reviewer.profile.profile_id(),
            fixer.profile.profile_id(),
        );
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        let request = |run_id, text: &str| {
            ProductRunRequest::new(run_id, workspace, providers, text.to_owned()).expect("request")
        };
        let first = RunId::new([0x55; 16]).expect("first run");
        service
            .start_interaction(
                request(first, "First question"),
                ProductInteractionMode::Chat,
                ProductRoleModels::default(),
            )
            .await
            .expect("first chat");
        assert_eq!(
            wait_for_terminal(&service, first).await.phase(),
            ProductRunPhase::WaitingForUser
        );

        let second = RunId::new([0x56; 16]).expect("second run");
        service
            .start_interaction(
                request(second, "Second question"),
                ProductInteractionMode::Chat,
                ProductRoleModels::default(),
            )
            .await
            .expect("new idle chat");
        assert_eq!(
            wait_for_terminal(&service, second).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        assert_eq!(
            service
                .query_interaction(ProductInteractionQuery::new(first))
                .expect("retained first conversation")
                .snapshot()
                .phase(),
            ProductRunPhase::WaitingForUser
        );
        service.shutdown().await.expect("shutdown product runs");
    });
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
            .start_interaction(request, ProductInteractionMode::Chat, ProductRoleModels::default())
            .await
            .expect("start");
        let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = service
                    .query_interaction(ProductInteractionQuery::new(run_id))
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
        service.shutdown().await.expect("shutdown product runs");
    });
}

#[path = "interaction/read_only.rs"]
mod read_only;

fn assert_recovery_notice_persisted(
    service: &ProductRunService,
    run: RunId,
    activities: &[peritus_app_protocol::ProductActivity],
) {
    let notices = activities
        .iter()
        .filter(|activity| activity.detail() == "Host recovery notice")
        .collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    let notice = notices[0];
    assert_eq!(notice.kind(), ProductActivityKind::Assistant);
    assert!(notice.text().contains("attempt 2 of 3"));
    assert!(notice.text().contains("invalid review"));
    let records = service.load_test_records().expect("durable records");
    let record = records.get(&run).expect("run record");
    assert_eq!(record.progress.retries, 1);
    assert!(record.interaction.activities.contains(notice));
}
