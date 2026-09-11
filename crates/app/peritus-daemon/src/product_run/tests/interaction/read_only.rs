//! Read-only interaction modes must reject provider-requested writes.

use super::*;

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
