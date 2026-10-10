//! Authenticated file preview, durable receipt, retained version and restart path.
use super::*;
use peritus_app_protocol::{
    ArtifactChunk, ArtifactCompletion, ArtifactMetadata, CanonicalMediaType, ProductModelChoice,
    TransferId, WorkbenchFileImportRequest, WorkbenchFileMetadata, WorkbenchFileMode,
    WorkbenchFileQuery, WorkbenchFileRange, WorkbenchFileRequest, WorkbenchFileUpload,
    WorkbenchInputText,
};
use peritus_types::{ArtifactId, ProviderProfileId};

#[test]
fn file_confirmation_restart_replay_and_deselection_remain_local() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let source = temporary.path().join("reference.txt");
        std::fs::write(&source, "first\nsecond\nthird\n").expect("source");
        let runtime =
            DaemonRuntime::start(images::configuration(temporary.path(), 1)).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        images::create(&mut client).await;
        let selection = WorkbenchFileRequest::new(
            images::query(),
            1,
            "reference.txt".to_owned(),
            WorkbenchFileRange::Lines { first: 2, last: 2 },
            WorkbenchFileMode::Snapshot,
            ProviderProfileId::new([0x91; 16]).expect("profile"),
            ProductModelChoice::default(),
        )
        .expect("request");
        let response =
            request(&mut client, 2, AppRequestPayload::PreviewWorkbenchFile(selection.clone()))
                .await;
        let AppResponsePayload::WorkbenchFilePreview(preview) = response else {
            panic!("{response:?}")
        };
        assert_eq!(preview.file().digest(), peritus_codec::sha256(b"second\n"));
        assert_eq!(preview.file().range(), (6, 13));
        let confirm = images::command(
            93,
            1,
            WorkbenchIntent::AttachFile {
                preview,
                text: WorkbenchInputText::new("Use the selected second line".to_owned())
                    .expect("caption"),
            },
        );
        let receipt =
            request(&mut client, 3, AppRequestPayload::WorkbenchCommand(confirm.clone())).await;
        assert!(
            matches!(&receipt, AppResponsePayload::WorkbenchReceipt(receipt) if receipt.accepted_revision() == 2),
            "{receipt:?}"
        );
        let mut old_client = connect(runtime.endpoint_address().clone(), false).await;
        assert!(
            matches!(request(&mut old_client, 4, AppRequestPayload::PreviewWorkbenchFile(selection)).await, AppResponsePayload::Error(error) if error.code() == AppErrorCode::MissingRequiredFeature)
        );
        drop(old_client);
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        std::fs::write(&source, "changed after confirmation\n").expect("edit");
        let runtime = DaemonRuntime::start(images::configuration(temporary.path(), 2))
            .await
            .expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        for (sequence, payload) in [
            (1, AppRequestPayload::QueryWorkbenchReceipt(confirm.clone())),
            (2, AppRequestPayload::WorkbenchCommand(confirm)),
        ] {
            assert_eq!(request(&mut client, sequence, payload).await, receipt);
        }
        let response = request(
            &mut client,
            3,
            AppRequestPayload::QueryWorkbenchFiles(
                WorkbenchFileQuery::new(images::query(), 2, 0).expect("query"),
            ),
        )
        .await;
        let AppResponsePayload::WorkbenchFiles(page) = response else { panic!("{response:?}") };
        assert_eq!(page.rows().len(), 1);
        assert!(page.rows()[0].selected());
        assert_eq!(page.rows()[0].file().digest(), peritus_codec::sha256(b"second\n"));
        let deselect = images::command(
            94,
            2,
            WorkbenchIntent::SelectFile {
                attachment: page.rows()[0].attachment(),
                selected: false,
            },
        );
        assert!(matches!(
            request(&mut client, 4, AppRequestPayload::WorkbenchCommand(deselect)).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let response = request(
            &mut client,
            5,
            AppRequestPayload::QueryWorkbenchFiles(
                WorkbenchFileQuery::new(images::query(), 3, 0).expect("query"),
            ),
        )
        .await;
        assert!(
            matches!(response, AppResponsePayload::WorkbenchFiles(page) if !page.rows()[0].selected() && !page.rows()[0].eligible())
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}

async fn upload_external_text(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
) -> WorkbenchFileImportRequest {
    let selected = b"second\n";
    let artifact = ArtifactId::new([0xa1; 16]).expect("artifact");
    let metadata = ArtifactMetadata::new(
        TransferId::new([0xa2; 16]).expect("transfer"),
        artifact,
        selected.len() as u64,
        CanonicalMediaType::new("text/plain".to_owned(), 128).expect("MIME"),
        peritus_codec::sha256(selected),
        1024,
        1024,
    )
    .expect("metadata");
    for (sequence, payload) in [
        (
            2,
            AppRequestPayload::BeginWorkbenchFileUpload(
                WorkbenchFileUpload::new(images::query(), 1, metadata.clone()).expect("upload"),
            ),
        ),
        (
            3,
            AppRequestPayload::UploadArtifactChunk(
                ArtifactChunk::new(metadata.transfer_id(), artifact, 0, 0, selected.to_vec(), 1024)
                    .expect("chunk"),
            ),
        ),
        (
            4,
            AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
                metadata.transfer_id(),
                artifact,
                metadata.byte_size(),
                metadata.digest(),
            )),
        ),
    ] {
        assert!(
            matches!(request(client, sequence, payload).await, AppResponsePayload::Acknowledged(_)),
            "local upload step must be acknowledged"
        );
    }
    let source = b"first\nsecond\nthird\n";
    let selection = WorkbenchFileRequest::new(
        images::query(),
        1,
        "external-source.txt".to_owned(),
        WorkbenchFileRange::Lines { first: 2, last: 2 },
        WorkbenchFileMode::Snapshot,
        ProviderProfileId::new([0x91; 16]).expect("profile"),
        ProductModelChoice::default(),
    )
    .expect("selection");
    WorkbenchFileImportRequest::new(
        selection,
        artifact,
        WorkbenchFileMetadata::new(
            peritus_codec::sha256(source),
            source.len() as u64,
            (6, 13),
            peritus_codec::sha256(selected),
        )
        .expect("file metadata"),
    )
    .expect("import")
}

#[test]
fn external_text_upload_preview_confirm_and_restart_never_reopen_the_source_label() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(images::configuration(temporary.path(), 1)).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        images::create(&mut client).await;
        let import = upload_external_text(&mut client).await;
        let source = b"first\nsecond\nthird\n";
        let response =
            request(&mut client, 5, AppRequestPayload::PreviewWorkbenchFileImport(import)).await;
        let AppResponsePayload::WorkbenchFileImportPreview(preview) = response else {
            panic!("preview: {response:?}")
        };
        assert_eq!(preview.request().file().source_digest(), peritus_codec::sha256(source));
        assert_eq!(preview.resolved_model(), "fixture-vision");
        let confirm = images::command(
            0xa3,
            1,
            WorkbenchIntent::AttachFileImport {
                preview,
                text: WorkbenchInputText::new("Use the selected external line".to_owned())
                    .expect("caption"),
            },
        );
        let receipt =
            request(&mut client, 6, AppRequestPayload::WorkbenchCommand(confirm.clone())).await;
        assert!(
            matches!(&receipt, AppResponsePayload::WorkbenchReceipt(value) if value.accepted_revision() == 2),
            "{receipt:?}"
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");

        let source_named_like_the_label = temporary.path().join("external-source.txt");
        std::fs::write(&source_named_like_the_label, b"must never replace imported bytes")
            .expect("decoy");
        let runtime = DaemonRuntime::start(images::configuration(temporary.path(), 2))
            .await
            .expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_eq!(
            request(&mut client, 1, AppRequestPayload::QueryWorkbenchReceipt(confirm),).await,
            receipt
        );
        let response = request(
            &mut client,
            2,
            AppRequestPayload::QueryWorkbenchFiles(
                WorkbenchFileQuery::new(images::query(), 2, 0).expect("query"),
            ),
        )
        .await;
        let AppResponsePayload::WorkbenchFiles(page) = response else { panic!("{response:?}") };
        assert_eq!(page.rows()[0].label(), "external-source.txt");
        assert_eq!(page.rows()[0].file().digest(), peritus_codec::sha256(b"second\n"));
        assert_eq!(page.rows()[0].mode(), WorkbenchFileMode::Snapshot);
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}

async fn upload_message_part(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    sequence: &mut u8,
    identity: u8,
    label: &str,
    selected: &[u8],
) -> peritus_app_protocol::WorkbenchFileImportPreview {
    let artifact = ArtifactId::new([identity; 16]).unwrap();
    let transfer = TransferId::new([identity; 16]).unwrap();
    let digest = peritus_codec::sha256(selected);
    let metadata = ArtifactMetadata::new(
        transfer,
        artifact,
        selected.len() as u64,
        CanonicalMediaType::new("text/plain".to_owned(), 128).unwrap(),
        digest,
        32768,
        32768,
    )
    .unwrap();
    let mut payloads = vec![AppRequestPayload::BeginWorkbenchFileUpload(
        WorkbenchFileUpload::new(images::query(), 1, metadata).unwrap(),
    )];
    for (index, chunk) in selected.chunks(32768).enumerate() {
        payloads.push(AppRequestPayload::UploadArtifactChunk(
            ArtifactChunk::new(
                transfer,
                artifact,
                index as u64,
                (index * 32768) as u64,
                chunk.to_vec(),
                32768,
            )
            .unwrap(),
        ));
    }
    payloads.push(AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
        transfer,
        artifact,
        selected.len() as u64,
        digest,
    )));
    for payload in payloads {
        let response = request(client, *sequence, payload).await;
        *sequence += 1;
        assert!(matches!(response, AppResponsePayload::Acknowledged(_)), "{response:?}");
    }
    let selection = WorkbenchFileRequest::new(
        images::query(),
        1,
        label.to_owned(),
        WorkbenchFileRange::All,
        WorkbenchFileMode::Snapshot,
        ProviderProfileId::new([0x91; 16]).unwrap(),
        ProductModelChoice::default(),
    )
    .unwrap();
    let request_value = WorkbenchFileImportRequest::new(
        selection,
        artifact,
        WorkbenchFileMetadata::new(
            digest,
            selected.len() as u64,
            (0, selected.len() as u64),
            digest,
        )
        .unwrap(),
    )
    .unwrap();
    let response =
        request(client, *sequence, AppRequestPayload::PreviewWorkbenchFileImport(request_value))
            .await;
    *sequence += 1;
    let AppResponsePayload::WorkbenchFileImportPreview(preview) = response else {
        panic!("{response:?}")
    };
    preview
}

#[test]
fn large_message_and_attachment_admit_once_atomically_across_sessions_and_restart() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(images::configuration(temporary.path(), 1)).await.unwrap();
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        images::create(&mut client).await;
        let text = "User's full exact instructions λ界\n".repeat(10_000);
        let data = "Attachment source data\n".repeat(10_000);
        let mut sequence = 2;
        let message =
            upload_message_part(&mut client, &mut sequence, 0xb1, "User message", text.as_bytes())
                .await;
        let attachment =
            upload_message_part(&mut client, &mut sequence, 0xb2, "external.txt", data.as_bytes())
                .await;
        drop(client);
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        let invalid = mismatched_preview(&attachment, data.len() as u64);
        let bad = images::command(
            0xb3,
            1,
            WorkbenchIntent::EnqueueMessageBundle {
                text: WorkbenchInputText::new("Read full message and attachment".into()).unwrap(),
                message: Some(message.clone()),
                attachments: vec![invalid],
            },
        );
        assert!(matches!(
            request(&mut client, 1, AppRequestPayload::WorkbenchCommand(bad)).await,
            AppResponsePayload::Error(_)
        ));
        let empty = request(
            &mut client,
            2,
            AppRequestPayload::QueryWorkbenchQueue(
                peritus_app_protocol::WorkbenchQueueQuery::new(images::query(), 1, 0, false)
                    .unwrap(),
            ),
        )
        .await;
        assert!(matches!(empty, AppResponsePayload::WorkbenchQueue(page) if page.total() == 0));
        let confirm = images::command(
            0xb4,
            1,
            WorkbenchIntent::EnqueueMessageBundle {
                text: WorkbenchInputText::new("Read full message and attachment".into()).unwrap(),
                message: Some(message),
                attachments: vec![attachment],
            },
        );
        let receipt =
            request(&mut client, 3, AppRequestPayload::WorkbenchCommand(confirm.clone())).await;
        assert!(
            matches!(&receipt, AppResponsePayload::WorkbenchReceipt(receipt) if receipt.accepted_revision() == 2),
            "{receipt:?}"
        );
        let queued = request(
            &mut client,
            4,
            AppRequestPayload::QueryWorkbenchQueue(
                peritus_app_protocol::WorkbenchQueueQuery::new(images::query(), 2, 0, false)
                    .unwrap(),
            ),
        )
        .await;
        assert!(matches!(queued, AppResponsePayload::WorkbenchQueue(page) if page.total() == 1));
        drop(client);
        runtime.shutdown().await.unwrap();
        let runtime =
            DaemonRuntime::start(images::configuration(temporary.path(), 2)).await.unwrap();
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_eq!(
            request(&mut client, 1, AppRequestPayload::WorkbenchCommand(confirm.clone())).await,
            receipt
        );
        assert_eq!(
            request(&mut client, 2, AppRequestPayload::QueryWorkbenchReceipt(confirm)).await,
            receipt
        );
        let files = request(
            &mut client,
            3,
            AppRequestPayload::QueryWorkbenchFiles(
                WorkbenchFileQuery::new(images::query(), 2, 0).unwrap(),
            ),
        )
        .await;
        let AppResponsePayload::WorkbenchFiles(page) = files else { panic!("{files:?}") };
        assert_eq!(page.rows().len(), 2);
        let digests = page.rows().iter().map(|row| row.file().digest()).collect::<Vec<_>>();
        assert!(digests.contains(&peritus_codec::sha256(text.as_bytes())));
        assert!(digests.contains(&peritus_codec::sha256(data.as_bytes())));
        drop(client);
        runtime.shutdown().await.unwrap();
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}

fn mismatched_preview(
    attachment: &peritus_app_protocol::WorkbenchFileImportPreview,
    length: u64,
) -> peritus_app_protocol::WorkbenchFileImportPreview {
    let wrong = WorkbenchFileMetadata::new(
        peritus_types::Sha256Digest::new([99; 32]),
        length,
        (0, length),
        peritus_types::Sha256Digest::new([99; 32]),
    )
    .unwrap();
    peritus_app_protocol::WorkbenchFileImportPreview::new(
        WorkbenchFileImportRequest::new(
            attachment.request().selection().clone(),
            attachment.request().artifact(),
            wrong,
        )
        .unwrap(),
        attachment.provider_revision(),
        attachment.resolved_model().to_owned(),
    )
    .unwrap()
}
