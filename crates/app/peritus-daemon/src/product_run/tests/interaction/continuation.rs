use super::*;

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
        let barrier = super::super::super::execution::inject_finish_barrier(run_id);
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
fn failed_follow_up_persistence_leaves_no_duplicate_in_memory_input() {
    block_on(async {
        use super::super::super::persistence::{PersistenceFaultPoint, inject_persistence_fault};

        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x70,
            "chat",
            vec![text_response(b"First reply."), text_response(b"Second reply.")],
        );
        let reviewer = scripted(0x71, "review", Vec::new());
        let fixer = scripted(0x72, "fix", Vec::new());
        let workspace_id = WorkspaceId::new([0x73; 16]).expect("workspace");
        let run_id = RunId::new([0x74; 16]).expect("run");
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
        service
            .interact(ProductInteractionRequest::new(
                request,
                ProductInteractionMode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start chat");
        assert_eq!(
            wait_for_terminal(&service, run_id).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        let follow_up =
            ProductRunContinuation::new(run_id, "Only once".to_owned()).expect("continuation");
        inject_persistence_fault(
            &service.inner.directory,
            run_id,
            PersistenceFaultPoint::BeforeWrite,
        );

        assert!(service.continue_run(&follow_up).await.is_err());
        let unchanged = service
            .query_interaction(ProductRunConversationQuery::new(run_id))
            .expect("unchanged conversation");
        assert_eq!(unchanged.received(), 1);
        assert_eq!(
            unchanged
                .activities()
                .iter()
                .filter(|activity| activity.kind() == ProductActivityKind::User)
                .count(),
            1
        );

        service.continue_run(&follow_up).await.expect("retry follow-up");
        assert_eq!(
            wait_for_terminal(&service, run_id).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        let completed = service
            .query_interaction(ProductRunConversationQuery::new(run_id))
            .expect("completed conversation");
        assert_eq!((completed.received(), completed.incorporated()), (2, 2));
        assert_eq!(
            completed
                .activities()
                .iter()
                .filter(|activity| {
                    activity.kind() == ProductActivityKind::User && activity.text() == "Only once"
                })
                .count(),
            1
        );
        service.shutdown(Duration::from_secs(5)).await;
    });
}
