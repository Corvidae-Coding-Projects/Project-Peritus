//! Deterministic interleavings at the continuation's actual mode and launch boundaries.
use super::*;

struct Fixture {
    service: ProductRunService,
    writer: Arc<ScriptedProvider>,
    workspace: WorkspaceId,
    run: RunId,
    _repository: tempfile::TempDir,
    _state: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x51,
            "chat",
            vec![
                support::text_response(b"Initial answer."),
                support::text_response(b"Continued answer."),
            ],
        );
        let reviewer = scripted(0x52, "review", Vec::new());
        let fixer = scripted(0x53, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x54; 16]).expect("workspace");
        let run = RunId::new([0x55; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        assert!(matches!(
            service
                .workbench_command(actor(), &start(workspace, run, [&writer, &reviewer, &fixer]))
                .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        wait_for_terminal(&service, run).await;
        Self { service, writer, workspace, run, _repository: repository, _state: state }
    }
    async fn change(&self, id: u8, intent: WorkbenchIntent) -> WorkbenchCommand {
        let AppResponsePayload::WorkbenchExecution(state) =
            self.service.workbench_execution(actor(), query(self.workspace))
        else {
            panic!("execution")
        };
        let command = command(self.workspace, id, state.snapshot().revision(), intent);
        assert!(matches!(
            self.service.workbench_command(actor(), &command).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        command
    }
    async fn enqueue(&self, id: u8) -> WorkbenchCommand {
        self.change(
            id,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    WorkbenchInputId::new([id; 16]).expect("input"),
                    WorkbenchInputText::new(format!("EXACT ORIGINAL MESSAGE {id}")).expect("text"),
                    WorkbenchInputOrder::new(Vec::new()).expect("order"),
                )
                .expect("input"),
            )),
        )
        .await
    }
    fn selection(id: u8) -> WorkbenchInputSelection {
        WorkbenchInputSelection::new(WorkbenchInputId::new([id; 16]).expect("input"), 1)
            .expect("selection")
    }
    fn assert_not_launched(&self) {
        assert_eq!(self.writer.requests.lock().expect("requests").len(), 1);
        assert!(
            self.service.inner.records.read().expect("records")[&self.run]
                .message_launches
                .is_empty()
        );
    }
    async fn retry(
        &self,
        command: &WorkbenchCommand,
        mode: ProductInteractionMode,
    ) -> Result<peritus_app_protocol::ProductRunSnapshot, crate::product_run::ProductRunServiceError>
    {
        self.service
            .retry_bound(self.run, None, Some((command.operation().into_bytes(), mode.tag())))
            .await
    }
}

#[test]
fn bound_continuation_rejects_a_different_prepared_mode_before_launch_and_stays_idempotent() {
    interaction::block_on(async {
        let fixture = Fixture::new().await;
        let original = fixture.enqueue(88).await;
        // Request A prepared Chat. Request B then prepared Plan before A acquired the launch lock.
        fixture
            .service
            .prepare_conversation_mode(fixture.run, ProductInteractionMode::Chat)
            .await
            .expect("prepare A");
        fixture
            .service
            .prepare_conversation_mode(fixture.run, ProductInteractionMode::Plan)
            .await
            .expect("prepare B");
        let before = fixture
            .service
            .query_interaction(ProductInteractionQuery::new(fixture.run))
            .expect("before stale admission");
        assert!(matches!(
            fixture.retry(&original, ProductInteractionMode::Chat).await,
            Err(crate::product_run::ProductRunServiceError::InvalidState)
        ));
        fixture.assert_not_launched();
        assert_eq!(
            fixture
                .service
                .query_interaction(ProductInteractionQuery::new(fixture.run))
                .expect("after rejection"),
            before
        );
        fixture.retry(&original, ProductInteractionMode::Plan).await.expect("exact mode wins");
        fixture
            .retry(&original, ProductInteractionMode::Plan)
            .await
            .expect("concurrent retry observes admitted launch");
        wait_for_terminal(&fixture.service, fixture.run).await;
        fixture
            .retry(&original, ProductInteractionMode::Plan)
            .await
            .expect("completed retry observes admitted launch");
        assert_eq!(fixture.writer.requests.lock().expect("requests").len(), 2);
        assert_eq!(
            fixture.service.inner.records.read().expect("records")[&fixture.run].message_launches,
            vec![(original.operation().into_bytes(), ProductInteractionMode::Plan.tag())]
        );
        assert_eq!(
            fixture
                .service
                .query_interaction(ProductInteractionQuery::new(fixture.run))
                .expect("mode")
                .mode(),
            ProductInteractionMode::Plan
        );
        fixture.service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn original_submission_cannot_launch_an_edited_message() {
    interaction::block_on(async {
        let fixture = Fixture::new().await;
        let original = fixture.enqueue(88).await;
        fixture
            .change(
                90,
                WorkbenchIntent::Queue(WorkbenchQueueIntent::Edit {
                    selected: Fixture::selection(88),
                    text: WorkbenchInputText::new(
                        "REPLACEMENT NOT AUTHORIZED BY ORIGINAL SEND".to_owned(),
                    )
                    .expect("replacement"),
                }),
            )
            .await;
        let response = fixture
            .service
            .continue_workbench_execution(
                actor(),
                peritus_app_protocol::WorkbenchContinuation::bound(
                    query(fixture.workspace),
                    ProductInteractionMode::Plan,
                    original.operation(),
                ),
            )
            .await;
        assert!(matches!(response, AppResponsePayload::Interaction(_)));
        fixture.assert_not_launched();
        assert_eq!(
            fixture
                .service
                .query_interaction(ProductInteractionQuery::new(fixture.run))
                .expect("untouched mode")
                .mode(),
            ProductInteractionMode::Chat
        );
        fixture.service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn held_or_withdrawn_original_cannot_launch_another_pending_input_after_preparation() {
    interaction::block_on(async {
        let fixture = Fixture::new().await;
        let original = fixture.enqueue(88).await;
        let other = fixture.enqueue(89).await;
        fixture
            .service
            .prepare_conversation_mode(fixture.run, ProductInteractionMode::Plan)
            .await
            .expect("preparation before concurrent queue change");
        fixture
            .change(
                90,
                WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold {
                    selected: Fixture::selection(88),
                    held: true,
                }),
            )
            .await;
        assert!(fixture.retry(&original, ProductInteractionMode::Plan).await.is_err());
        fixture.assert_not_launched();
        fixture
            .change(
                91,
                WorkbenchIntent::Queue(WorkbenchQueueIntent::Withdraw(Fixture::selection(88))),
            )
            .await;
        assert!(fixture.retry(&original, ProductInteractionMode::Plan).await.is_err());
        fixture.assert_not_launched();
        fixture
            .retry(&other, ProductInteractionMode::Plan)
            .await
            .expect("other exact admission remains usable");
        wait_for_terminal(&fixture.service, fixture.run).await;
        let requests = fixture.writer.requests.lock().expect("requests").clone();
        assert_eq!(requests.len(), 2);
        let bytes = requests[1].canonical_bytes().expect("provider request");
        assert!(
            bytes
                .windows(b"EXACT ORIGINAL MESSAGE 89".len())
                .any(|part| part == b"EXACT ORIGINAL MESSAGE 89")
        );
        assert!(
            !bytes
                .windows(b"EXACT ORIGINAL MESSAGE 88".len())
                .any(|part| part == b"EXACT ORIGINAL MESSAGE 88")
        );
        assert_eq!(
            fixture.service.inner.records.read().expect("records")[&fixture.run].message_launches,
            vec![(other.operation().into_bytes(), ProductInteractionMode::Plan.tag())]
        );
        fixture.service.shutdown(Duration::from_secs(5)).await;
    });
}
