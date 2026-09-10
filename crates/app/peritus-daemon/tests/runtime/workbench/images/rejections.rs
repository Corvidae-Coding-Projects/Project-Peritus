use super::*;

fn confirm(preview: WorkbenchImagePreview) -> WorkbenchCommand {
    command(
        93,
        1,
        WorkbenchIntent::AttachImage {
            preview,
            text: WorkbenchInputText::new("Only the preview I confirmed".to_owned())
                .expect("caption"),
        },
    )
}

#[test]
fn unnegotiated_image_controls_reject_before_upload_or_preview_dispatch() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(configuration(temporary.path(), 1)).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        create(&mut client).await;
        let artifact = upload(&mut client, 92, GIF, "image/gif").await;
        let preview = preview(&mut client, artifact).await;
        drop(client);
        let mut client = connect(runtime.endpoint_address().clone(), false).await;
        let metadata = ArtifactMetadata::new(
            TransferId::new([94; 16]).expect("transfer"),
            ArtifactId::new([94; 16]).expect("artifact"),
            GIF.len() as u64,
            CanonicalMediaType::new("image/gif".to_owned(), 128).expect("MIME"),
            peritus_codec::sha256(GIF),
            1024,
            1024,
        )
        .expect("metadata");
        let confirm = confirm(preview);
        for (sequence, payload) in [
            (
                1,
                AppRequestPayload::BeginWorkbenchImageUpload(
                    WorkbenchImageUpload::new(query(), 1, metadata).expect("upload"),
                ),
            ),
            (2, AppRequestPayload::PreviewWorkbenchImage(selection(artifact, 1))),
            (3, AppRequestPayload::WorkbenchCommand(confirm.clone())),
            (4, AppRequestPayload::QueryWorkbenchReceipt(confirm)),
            (
                5,
                AppRequestPayload::QueryWorkbenchImages(
                    peritus_app_protocol::WorkbenchImageQuery::new(query(), 0, 0)
                        .expect("page query"),
                ),
            ),
        ] {
            assert_error(
                request(&mut client, sequence, payload).await,
                AppErrorCode::MissingRequiredFeature,
            );
        }
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}

#[test]
fn corrupt_pixels_wrong_mime_and_foreign_conversation_never_produce_a_preview() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(configuration(temporary.path(), 1)).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        create(&mut client).await;
        for (id, bytes, mime) in [(92, b"GIF89a".as_slice(), "image/gif"), (94, GIF, "image/png")] {
            let artifact = upload(&mut client, id, bytes, mime).await;
            assert_error(
                request(
                    &mut client,
                    5,
                    AppRequestPayload::PreviewWorkbenchImage(selection(artifact, 1)),
                )
                .await,
                AppErrorCode::MalformedFrame,
            );
        }
        // Distinct encoded content avoids deliberately conflicting MIME in the content-addressed store.
        let mut valid = GIF.to_vec();
        valid.push(0);
        let artifact = upload(&mut client, 95, &valid, "image/gif").await;
        let other_query =
            WorkbenchQuery::new(ConversationId::new([96; 16]).expect("other"), query().workspace());
        let create = WorkbenchCommand::new(
            ControlOperationId::new([96; 16]).expect("id"),
            other_query,
            0,
            WorkbenchIntent::CreateConversation(
                ConversationTitle::new("Other conversation".to_owned()).expect("title"),
            ),
        );
        assert!(matches!(
            request(&mut client, 6, AppRequestPayload::WorkbenchCommand(create)).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let own = selection(artifact, 1);
        let foreign = WorkbenchImageRequest::new(
            other_query,
            1,
            artifact,
            own.provider(),
            own.model().clone(),
            own.label().clone(),
        )
        .expect("foreign request");
        assert_error(
            request(&mut client, 7, AppRequestPayload::PreviewWorkbenchImage(foreign)).await,
            AppErrorCode::ReadOnly,
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}

#[test]
fn provider_revision_change_invalidates_unconfirmed_preview_but_allows_a_fresh_preview() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(configuration(temporary.path(), 1)).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        create(&mut client).await;
        let artifact = upload(&mut client, 92, GIF, "image/gif").await;
        let before = preview(&mut client, artifact).await;
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        let runtime =
            DaemonRuntime::start(configuration(temporary.path(), 2)).await.expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        let old = confirm(before);
        assert_error(
            request(&mut client, 1, AppRequestPayload::WorkbenchCommand(old.clone())).await,
            AppErrorCode::StaleRevision,
        );
        assert_error(
            request(&mut client, 2, AppRequestPayload::QueryWorkbenchReceipt(old)).await,
            AppErrorCode::InvalidIdentifier,
        );
        let current = preview(&mut client, artifact).await;
        assert_eq!(current.provider_revision(), 2);
        assert!(matches!(
            request(&mut client, 6, AppRequestPayload::WorkbenchCommand(confirm(current))).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}

#[test]
fn forged_preview_and_mismatched_outer_revision_reject_without_consuming_the_image() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(configuration(temporary.path(), 1)).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        create(&mut client).await;
        let artifact = upload(&mut client, 92, GIF, "image/gif").await;
        let original = preview(&mut client, artifact).await;
        let forged = WorkbenchImagePreview::new(
            original.request().clone(),
            original.image(),
            1,
            "another-model".to_owned(),
        )
        .expect("forged");
        assert_error(
            request(&mut client, 6, AppRequestPayload::WorkbenchCommand(confirm(forged))).await,
            AppErrorCode::StaleRevision,
        );
        let exact = confirm(original);
        let mismatch = command(93, 2, exact.intent().clone());
        assert_error(
            request(&mut client, 7, AppRequestPayload::WorkbenchCommand(mismatch)).await,
            AppErrorCode::SessionMismatch,
        );
        let rename = command(
            94,
            1,
            WorkbenchIntent::RenameConversation(
                ConversationTitle::new("Changed meanwhile".to_owned()).expect("title"),
            ),
        );
        assert!(matches!(
            request(&mut client, 8, AppRequestPayload::WorkbenchCommand(rename)).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_error(
            request(&mut client, 9, AppRequestPayload::WorkbenchCommand(exact.clone())).await,
            AppErrorCode::StaleRevision,
        );
        assert_error(
            request(&mut client, 10, AppRequestPayload::QueryWorkbenchReceipt(exact)).await,
            AppErrorCode::InvalidIdentifier,
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}
