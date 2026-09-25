//! Collection is passive; explicit selection uses the actual product-run launch boundary.
use super::*;
use peritus_app_protocol::{ImprovementRequest, ImprovementText};

#[tokio::test]
async fn suggestions_require_real_terminal_evidence_and_only_explicit_evaluation_launches() {
    let repo = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x91, "writer", Vec::new());
    let reviewer = scripted(0x92, "reviewer", Vec::new());
    let fixer = scripted(0x93, "fixer", Vec::new());
    let workspace = WorkspaceId::new([0x94; 16]).expect("workspace");
    let source = RunId::new([0x95; 16]).expect("source");
    let service = service(state.path(), repo.path(), workspace, [&writer, &reviewer, &fixer]);
    let proposal = ImprovementText::new("Investigate response validation".into()).expect("text");
    assert!(
        service
            .improvements(&ImprovementRequest::Suggest {
                workspace,
                run: source,
                proposal: proposal.clone()
            })
            .await
            .is_err()
    );
    assert!(
        service
            .improvements(&ImprovementRequest::List(workspace))
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
        .improvements(&ImprovementRequest::Suggest { workspace, run: source, proposal })
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
        run: ProductRunRequest::new(
            evaluation,
            workspace,
            providers,
            "untrusted task ignored".into(),
        )
        .expect("request"),
    };
    assert!(
        service.improvements(&request).await.is_err(),
        "ordinary project is not harness source"
    );
    assert_eq!(service.inner.records.read().expect("records").len(), 1);
    fs::write(repo.path().join("Cargo.toml"), "[workspace]\n[workspace.metadata.peritus]\n")
        .expect("source marker");
    let collision = ImprovementRequest::Evaluate {
        workspace,
        candidate,
        run: ProductRunRequest::new(source, workspace, providers, "Evaluate".into())
            .expect("collision request"),
    };
    assert!(
        service.improvements(&collision).await.is_err(),
        "cannot adopt an unrelated existing run"
    );
    assert!(
        service
            .improvements(&ImprovementRequest::List(workspace))
            .await
            .expect("inbox")
            .candidates()
            .iter()
            .all(|c| c.evaluation().is_none())
    );
    let first = service.improvements(&request).await.expect("explicit launch");
    assert_eq!(
        first.candidates().iter().find(|c| c.id() == candidate).expect("candidate").evaluation(),
        Some(evaluation)
    );
    service.improvements(&request).await.expect("idempotent repeat");
    assert_eq!(service.inner.records.read().expect("records").len(), 2);
    {
        let records = service.inner.records.read().expect("records");
        let record = records.get(&evaluation).expect("launched record");
        assert!(
            record
                .request
                .task()
                .contains("Run the regression against the baseline before the fix")
        );
        assert!(!record.request.task().contains("untrusted task ignored"));
    }
    service.shutdown(Duration::from_secs(5)).await;
}
