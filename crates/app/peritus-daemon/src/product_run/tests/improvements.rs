//! Collection is passive; explicit selection uses the actual product-run launch boundary.
use super::*;
use peritus_app_protocol::{
    AppResponsePayload, ImprovementEvaluationRequest, ImprovementRequest, ImprovementText,
    WorkbenchQuery, WorkbenchQueueQuery,
};
use peritus_types::ActorId;

#[tokio::test]
async fn suggestions_require_real_terminal_evidence_and_only_explicit_evaluation_launches() {
    let repo = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x91, "writer", Vec::new());
    let reviewer = scripted(0x92, "reviewer", Vec::new());
    let fixer = scripted(0x93, "fixer", Vec::new());
    let workspace = WorkspaceId::new([0x94; 16]).expect("workspace");
    let source = RunId::new([0x95; 16]).expect("source");
    let actor = ActorId::new([0x97; 16]).expect("actor");
    let service = service(state.path(), repo.path(), workspace, [&writer, &reviewer, &fixer]);
    let proposal = ImprovementText::new("Investigate response validation".into()).expect("text");
    assert!(
        service
            .improvements(
                actor,
                &ImprovementRequest::Suggest { workspace, run: source, proposal: proposal.clone() }
            )
            .await
            .is_err()
    );
    assert!(
        service
            .improvements(actor, &ImprovementRequest::List(workspace))
            .await
            .expect("empty")
            .candidates()
            .is_empty()
    );
    let providers = ProductProviderSelection::new(
        writer.profile.profile_id(),
        reviewer.profile.profile_id(),
        fixer.profile.profile_id(),
    );
    service
        .start(
            ProductRunRequest::new(source, workspace, providers, "Inspect the repository".into())
                .expect("request"),
        )
        .await
        .expect("source start");
    let _ = wait_for_terminal(&service, source).await;
    let inbox = service
        .improvements(actor, &ImprovementRequest::Suggest { workspace, run: source, proposal })
        .await
        .expect("collect");
    assert!(inbox.candidates().iter().all(|c| c.evaluation().is_none()));
    assert_eq!(service.inner.records.read().expect("records").len(), 1);
    let candidate = inbox
        .candidates()
        .iter()
        .find(|c| c.proposal().as_str() == "Investigate response validation")
        .expect("manual candidate")
        .id();
    let evaluation = RunId::new([0x96; 16]).expect("evaluation");
    let request = ImprovementRequest::Evaluate {
        workspace,
        candidate,
        evaluation: ImprovementEvaluationRequest::new(evaluation, workspace, providers),
    };
    assert!(
        service.improvements(actor, &request).await.is_err(),
        "ordinary project is not harness source"
    );
    assert_eq!(service.inner.records.read().expect("records").len(), 1);
    fs::write(repo.path().join("Cargo.toml"), "[workspace]\n[workspace.metadata.peritus]\n")
        .expect("source marker");
    let collision = ImprovementRequest::Evaluate {
        workspace,
        candidate,
        evaluation: ImprovementEvaluationRequest::new(source, workspace, providers),
    };
    assert!(
        service.improvements(actor, &collision).await.is_err(),
        "cannot adopt an unrelated existing run"
    );
    assert!(
        service
            .improvements(actor, &ImprovementRequest::List(workspace))
            .await
            .expect("inbox")
            .candidates()
            .iter()
            .all(|c| c.evaluation().is_none())
    );
    let first = service.improvements(actor, &request).await.expect("explicit launch");
    let route = first
        .candidates()
        .iter()
        .find(|c| c.id() == candidate)
        .expect("candidate")
        .evaluation()
        .expect("evaluation route");
    assert_eq!(route.run(), evaluation);
    assert_eq!(route.target(), workspace);
    service.improvements(actor, &request).await.expect("idempotent repeat");
    assert_eq!(service.inner.records.read().expect("records").len(), 2);
    {
        let records = service.inner.records.read().expect("records");
        let record = records.get(&evaluation).expect("launched record");
        assert_eq!(
            record.request.execution_task(),
            "Execute the selected durable workbench inputs."
        );
        assert!(
            record.snapshot.task().starts_with("Harness improvement "),
            "dashboard identity comes from the durable conversation title"
        );
        let operation = &record.interaction.workbench;
        assert_eq!(operation.conversation().as_bytes(), route.conversation().as_bytes());
    }
    let query = WorkbenchQuery::new(route.conversation(), route.target());
    let AppResponsePayload::WorkbenchQueue(queue) = service
        .workbench_queue(actor, WorkbenchQueueQuery::new(query, 0, 0, true).expect("queue query"))
    else {
        panic!("durable evaluation queue");
    };
    assert_eq!(queue.total(), 3);
    assert!(queue.rows().iter().any(|row| {
        row.text().as_str().contains("UNTRUSTED IMPROVEMENT CANDIDATE")
            && row.text().as_str().contains("Investigate response validation")
    }));
    assert!(queue.rows().iter().any(|row| {
        row.text().as_str().contains("UNTRUSTED RUN OBSERVATION")
            && row.text().as_str().contains("Peritus")
    }));
    let directive = queue
        .rows()
        .iter()
        .find(|row| row.text().as_str().starts_with("PERITUS HARNESS EVALUATION"))
        .expect("directive");
    assert_eq!(directive.dependencies().ids().len(), 2);
    service.shutdown().await.expect("shutdown product runs");
}
