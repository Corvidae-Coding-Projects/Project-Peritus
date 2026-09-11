//! Composed library behavior through the actual daemon service.

use super::*;

mod search;

#[test]
fn allocated_isolated_fork_preserves_parent_and_starts_as_an_independent_draft() {
    interaction::block_on(async {
        use peritus_product_runner::control::{
            BriefField, ControlIntent, ControlOperation, ControlText, GoalBudget, GoalCriterion,
            GoalCriterionKind, InputId, OperationId, QueueIntent,
        };

        let parent_repository = repository();
        let child_repository = repository();
        let parent_bytes = fs::read(parent_repository.path().join("src/lib.rs")).unwrap();
        let parent_head = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(parent_repository.path())
            .output()
            .unwrap()
            .stdout;
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(0x21, "unused-writer", Vec::new());
        let reviewer = scripted(0x22, "unused-reviewer", Vec::new());
        let fixer = scripted(0x23, "unused-fixer", Vec::new());
        let parent_workspace = WorkspaceId::new([0x24; 16]).unwrap();
        let child_workspace = WorkspaceId::new([0x25; 16]).unwrap();
        let mut service = service(
            state.path(),
            parent_repository.path(),
            parent_workspace,
            [&writer, &reviewer, &fixer],
        );
        Arc::get_mut(&mut service.inner)
            .unwrap()
            .workspaces
            .insert(child_workspace, child_repository.path().to_owned());
        let source = peritus_product_runner::control::ConversationId::new([2; 16]).unwrap();
        let objective = "Implement the isolated child change.";
        service
            .with_controls(true, |store| {
                let operation = |id, revision, intent| {
                    ControlOperation::new(
                        OperationId::new([id; 16]).unwrap(),
                        source,
                        actor(),
                        parent_workspace,
                        revision,
                        intent,
                    )
                };
                store.accept(&operation(
                    20,
                    0,
                    ControlIntent::CreateConversation {
                        title: ControlText::new("Parent conversation".to_owned()).unwrap(),
                    },
                ))?;
                store.accept(&operation(
                    21,
                    1,
                    ControlIntent::Queue(QueueIntent::Enqueue {
                        id: InputId::new([22; 16]).unwrap(),
                        text: ControlText::new(objective.to_owned()).unwrap(),
                        dependencies: Vec::new(),
                    }),
                ))?;
                store.accept(&operation(
                    23,
                    2,
                    ControlIntent::SetBrief {
                        field: BriefField::Objective,
                        text: ControlText::new(objective.to_owned()).unwrap(),
                    },
                ))?;
                store.accept(&operation(
                    24,
                    3,
                    ControlIntent::StartGoal {
                        run: [25; 16],
                        settings_digest: [26; 32],
                        objective: ControlText::new(objective.to_owned()).unwrap(),
                        criteria: vec![
                            GoalCriterion::new(
                                GoalCriterionKind::RunnerAcceptance,
                                "Strict runner acceptance".to_owned(),
                                true,
                            )
                            .unwrap(),
                        ],
                        budget: GoalBudget::new(Some(100_000), Some(5), Some(5), Some(1_000))
                            .unwrap(),
                        now_unix_millis: 1,
                    },
                ))?;
                Ok(())
            })
            .unwrap();

        let request = peritus_app_protocol::WorkbenchFileRequest::new(
            query(parent_workspace),
            4,
            "src/lib.rs".to_owned(),
            peritus_app_protocol::WorkbenchFileRange::All,
            peritus_app_protocol::WorkbenchFileMode::RefreshOnRequest,
            writer.profile.profile_id(),
            peritus_app_protocol::ProductModelChoice::default(),
        )
        .unwrap();
        let AppResponsePayload::WorkbenchFilePreview(file) =
            service.preview_workbench_file(actor(), &request).await
        else {
            panic!("file preview")
        };
        let attach = command(
            parent_workspace,
            0x2f,
            4,
            WorkbenchIntent::AttachFile {
                preview: file,
                text: WorkbenchInputText::new("Historical covered source".to_owned()).unwrap(),
            },
        );
        assert!(matches!(
            service.confirm_workbench_file(actor(), &attach).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let checkpoint = match service
            .workbench_command(
                actor(),
                &WorkbenchCommand::new(
                    ControlOperationId::new([0x30; 16]).unwrap(),
                    query(parent_workspace),
                    5,
                    WorkbenchIntent::CreateCheckpoint(
                        peritus_app_protocol::WorkbenchCheckpointName::new(
                            "Fork source".to_owned(),
                        )
                        .unwrap(),
                    ),
                ),
            )
            .await
        {
            AppResponsePayload::WorkbenchCheckpoint(receipt) => receipt,
            response => panic!("expected actual checkpoint, got {response:?}"),
        };
        service
            .with_controls(false, |store| {
                store.accept(&ControlOperation::new(
                    OperationId::new([0x31; 16]).unwrap(),
                    source,
                    actor(),
                    parent_workspace,
                    checkpoint.accepted_revision(),
                    ControlIntent::SetBrief {
                        field: BriefField::Objective,
                        text: ControlText::new("Later parent objective".to_owned()).unwrap(),
                    },
                ))?;
                Ok(())
            })
            .unwrap();
        let references = checkpoint.references();
        let child_query =
            WorkbenchQuery::new(ConversationId::new([0x27; 16]).unwrap(), child_workspace);
        let allocation = WorkbenchForkBudget::new(20_000, 2, 2, 400).unwrap();
        let fork = WorkbenchCommand::new(
            ControlOperationId::new([0x28; 16]).unwrap(),
            query(parent_workspace),
            checkpoint.accepted_revision() + 1,
            WorkbenchIntent::ForkConversation(
                WorkbenchForkRequest::new(
                    child_query,
                    ConversationTitle::new("Isolated child".to_owned()).unwrap(),
                    checkpoint.checkpoint(),
                    references.source_conversation_revision(),
                    references.context_generation(),
                    references.brief_revision(),
                    references.goal_revision().unwrap_or(0),
                    WorkbenchForkMode::IsolatedWritableWorkspace,
                    Some(allocation),
                )
                .unwrap(),
            ),
        );
        fs::write(child_repository.path().join("src/lib.rs"), b"unrelated child bytes\n").unwrap();
        assert!(matches!(
            service.workbench_command(actor(), &fork).await,
            AppResponsePayload::Error(_)
        ));
        fs::write(child_repository.path().join("src/lib.rs"), &parent_bytes).unwrap();
        let response = service.workbench_command(actor(), &fork).await;
        assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)), "{response:?}");
        service
            .with_controls(false, |store| {
                let parent = store.load(source)?.unwrap();
                let child = store
                    .load(peritus_product_runner::control::ConversationId::new([0x27; 16])?)?
                    .unwrap();
                let reservation = parent.goal().unwrap().child_budget_reservation();
                assert_eq!(
                    (
                        reservation.active_millis(),
                        reservation.requests(),
                        reservation.tool_calls(),
                        reservation.total_tokens(),
                    ),
                    (20_000, 2, 2, 400)
                );
                assert_eq!(parent.title(), "Parent conversation");
                assert_eq!(child.title(), "Isolated child");
                assert!(child.execution().is_none());
                assert!(child.goal().is_none());
                let historical = store
                    .load_revision(source, references.source_conversation_revision())?
                    .unwrap();
                assert_eq!(child.inputs().revisions().len(), historical.inputs().revisions().len());
                for (inherited, source) in
                    child.inputs().revisions().iter().zip(historical.inputs().revisions())
                {
                    assert_eq!(inherited.selection(), source.selection());
                    assert_eq!(inherited.text(), source.text());
                }
                assert!(child.inputs().capture()?.pending().is_empty());
                assert_eq!(child.files(), &historical.files().historical_snapshot());
                assert_eq!(child.context(), historical.context());
                assert_eq!(child.brief(), historical.brief());
                assert!(child.restores().is_empty());
                let branch = store.branch(child.id())?.unwrap();
                assert_eq!(branch.source(), parent.id());
                assert_eq!(branch.source_revision(), references.source_conversation_revision());
                assert_eq!(branch.objective(), Some(objective));
                Ok(())
            })
            .unwrap();
        let parent_revision = service
            .with_controls(false, |store| {
                Ok(store.load(source)?.ok_or(ControlError::NotFound)?.revision())
            })
            .unwrap();
        let read_only_child =
            WorkbenchQuery::new(ConversationId::new([0x36; 16]).unwrap(), parent_workspace);
        let read_only_allocation = WorkbenchForkBudget::new(10_000, 1, 1, 200).unwrap();
        let read_only_fork = WorkbenchCommand::new(
            ControlOperationId::new([0x35; 16]).unwrap(),
            query(parent_workspace),
            parent_revision,
            WorkbenchIntent::ForkConversation(
                WorkbenchForkRequest::new(
                    read_only_child,
                    ConversationTitle::new("Read-only governed child".to_owned()).unwrap(),
                    checkpoint.checkpoint(),
                    references.source_conversation_revision(),
                    references.context_generation(),
                    references.brief_revision(),
                    references.goal_revision().unwrap_or(0),
                    WorkbenchForkMode::ReadOnlyCurrentWorkspace,
                    Some(read_only_allocation),
                )
                .unwrap(),
            ),
        );
        assert!(matches!(
            service.workbench_command(actor(), &read_only_fork).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        service
            .with_controls(false, |store| {
                let parent = store.load(source)?.ok_or(ControlError::NotFound)?;
                let reservation = parent.goal().unwrap().child_budget_reservation();
                assert_eq!(
                    (
                        reservation.active_millis(),
                        reservation.requests(),
                        reservation.tool_calls(),
                        reservation.total_tokens(),
                    ),
                    (30_000, 3, 3, 600)
                );
                let child = store
                    .load(peritus_product_runner::control::ConversationId::new([0x36; 16])?)?
                    .ok_or(ControlError::NotFound)?;
                assert!(child.execution().is_none());
                assert!(child.goal().is_none());
                let branch = store.branch(child.id())?.ok_or(ControlError::NotFound)?;
                assert_eq!(
                    branch.mode(),
                    peritus_product_runner::control::ConversationBranchMode::ReadOnlyCurrentWorkspace
                );
                let allocation = branch.allocation().ok_or(ControlError::NotFound)?;
                assert_eq!(
                    (
                        allocation.active_millis(),
                        allocation.requests(),
                        allocation.tool_calls(),
                        allocation.total_tokens(),
                    ),
                    (10_000, 1, 1, 200)
                );
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            service
                .workbench_command(
                    actor(),
                    &WorkbenchCommand::new(
                        ControlOperationId::new([0x29; 16]).unwrap(),
                        child_query,
                        1,
                        WorkbenchIntent::RenameConversation(
                            ConversationTitle::new("Independent child title".to_owned()).unwrap(),
                        ),
                    ),
                )
                .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        fs::write(
            child_repository.path().join("src/lib.rs"),
            "pub const fn child_only() -> u32 { 42 }\n",
        )
        .unwrap();
        assert_eq!(fs::read(parent_repository.path().join("src/lib.rs")).unwrap(), parent_bytes);
        assert_eq!(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(parent_repository.path())
                .output()
                .unwrap()
                .stdout,
            parent_head
        );
        let page = match service.conversation_library(
            actor(),
            &ConversationLibraryQuery::new(child_workspace, None, true, 0, 16).unwrap(),
        ) {
            AppResponsePayload::ConversationLibrary(page) => page,
            response => panic!("expected child library page, got {response:?}"),
        };
        assert_eq!(page.items()[0].title().as_str(), "Independent child title");
        assert!(page.items()[0].goal_draft());
        assert_eq!(page.items()[0].branch().unwrap().allocation(), Some(allocation));
        assert!(writer.requests.lock().unwrap().is_empty());
        service.shutdown(Duration::from_secs(5)).await;
    });
}
