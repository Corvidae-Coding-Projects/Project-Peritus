use super::*;
use peritus_app_protocol::{AppErrorCode, WorkbenchImageQuery, WorkbenchQuery};

pub(super) fn inspect_scope_fences(service: &ProductRunService, workspace: WorkspaceId) {
    let scope = WorkbenchQuery::new(ConversationId::new([2; 16]).expect("conversation"), workspace);
    let query = WorkbenchImageQuery::new(scope, 0, 0).expect("latest");
    let original = service.workbench_images(actor(), query);
    let AppResponsePayload::WorkbenchImages(page) = &original else {
        panic!("images: {original:?}")
    };
    assert_eq!(page.total(), 1);
    assert_eq!(page.query().revision(), 4);
    for (selected_actor, query, expected) in [
        (ActorId::new([0x92; 16]).expect("other actor"), query, AppErrorCode::SessionMismatch),
        (
            actor(),
            WorkbenchImageQuery::new(
                WorkbenchQuery::new(
                    scope.conversation(),
                    WorkspaceId::new([0x93; 16]).expect("other workspace"),
                ),
                0,
                0,
            )
            .expect("foreign scope"),
            AppErrorCode::SessionMismatch,
        ),
        (
            actor(),
            WorkbenchImageQuery::new(scope, 3, 0).expect("stale"),
            AppErrorCode::StaleRevision,
        ),
        (
            actor(),
            WorkbenchImageQuery::new(scope, 4, 2).expect("beyond end"),
            AppErrorCode::MalformedFrame,
        ),
    ] {
        let AppResponsePayload::Error(error) = service.workbench_images(selected_actor, query)
        else {
            panic!("reject {expected:?}")
        };
        assert_eq!(error.code(), expected);
    }
    assert_eq!(
        service.workbench_images(actor(), query),
        original,
        "inspection/rejections must not mutate current state"
    );
}

#[test]
fn retained_image_pages_cover_more_than_one_page_at_one_exact_revision() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = support::scripted_images(0x51, "vision", vec![]);
        let reviewer = scripted(0x52, "review", vec![]);
        let fixer = scripted(0x53, "fix", vec![]);
        let workspace = WorkspaceId::new([0x54; 16]).expect("workspace");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        seed_history(&service, workspace, &writer.profile).await;
        let scope = query(workspace);
        let AppResponsePayload::WorkbenchImages(first) = service
            .workbench_images(actor(), WorkbenchImageQuery::new(scope, 0, 0).expect("first"))
        else {
            panic!("first page")
        };
        assert_eq!(first.total(), 33);
        assert_eq!(first.rows().len(), 32);
        assert_eq!(first.query().revision(), 69);
        assert!(first.rows().iter().all(|row| !row.selected() && !row.eligible()));
        let AppResponsePayload::WorkbenchImages(last) = service
            .workbench_images(actor(), WorkbenchImageQuery::new(scope, 69, 32).expect("last"))
        else {
            panic!("last page")
        };
        assert_eq!(last.rows().len(), 1);
        assert_eq!(last.rows()[0].operation().as_bytes(), &[42; 16]);
        assert!(!first.rows().iter().any(|row| row.operation() == last.rows()[0].operation()));
        let AppResponsePayload::WorkbenchImages(end) = service
            .workbench_images(actor(), WorkbenchImageQuery::new(scope, 69, 33).expect("end"))
        else {
            panic!("empty end")
        };
        assert!(end.rows().is_empty());
        assert!(writer.requests.lock().expect("requests").is_empty());
        service.shutdown(Duration::from_secs(5)).await;
    });
}

async fn seed_history(
    service: &ProductRunService,
    workspace: WorkspaceId,
    profile: &peritus_model_protocol::ProviderProfile,
) {
    let image = ValidatedImage::decode(
        vec![
            71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 44, 0, 0, 0, 0,
            1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
        ],
        profile,
    )
    .expect("image");
    for index in 0_u8..33 {
        let operation = OperationId::new([10 + index; 16]).expect("operation");
        let reference = ImageAttachment::from_validated(
            operation,
            ArtifactId::new([60 + index; 16]).expect("artifact"),
            ControlText::new(format!("history-{index}.gif")).expect("label"),
            &image,
        )
        .expect("reference");
        let import = ControlOperation::new(
            operation,
            peritus_product_runner::control::ConversationId::new([2; 16]).expect("conversation"),
            actor(),
            workspace,
            3 + 2 * u64::from(index),
            ControlIntent::AttachImage {
                image: reference,
                text: ControlText::new(format!("Caption {index}")).expect("caption"),
            },
        );
        service
            .with_controls(false, |store| store.accept_image(&import, &image))
            .expect("atomic image");
        let deselect = command(
            workspace,
            110 + index,
            4 + 2 * u64::from(index),
            WorkbenchIntent::SelectImage {
                attachment: ControlOperationId::new(*operation.as_bytes()).expect("id"),
                selected: false,
            },
        );
        assert!(matches!(
            service.workbench_command(actor(), &deselect).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
    }
}
