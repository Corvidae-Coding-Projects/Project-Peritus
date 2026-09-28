use super::*;

#[test]
fn escape_during_external_read_or_upload_prevents_continuation_without_losing_drafts() {
    for uploading in [false, true] {
        let mut model = opened();
        model.chat.workbench.files.path = if cfg!(windows) {
            r"C:\explicit\external.txt".into()
        } else {
            "/explicit/external.txt".into()
        };
        model.chat.workbench.files.caption = "Keep this instruction".into();
        let effects = refreshed_preview(&mut model);
        let [Effect::ReadFile { operation, .. }] = effects.as_slice() else { panic!("read") };
        let file = || FileBytes {
            bytes: b"text".to_vec(),
            file: WorkbenchFileMetadata::new(
                peritus_codec::sha256(b"text"),
                4,
                (0, 4),
                peritus_codec::sha256(b"text"),
            )
            .unwrap(),
            label: "external.txt".into(),
        };
        let sent = if uploading {
            Some(request(
                &model.update(Action::FileRead { operation: *operation, result: Ok(file()) }),
            ))
        } else {
            None
        };
        model.chat.buffer = "Retain this new draft".into();
        let effects = key(&mut model, KeyCode::Esc);
        assert!(!model.workbench_request_pending());
        if let Some(sent) = sent {
            let cancel = request(&effects);
            let AppRequestPayload::BeginWorkbenchFileUpload(upload) = sent.payload() else {
                panic!("upload")
            };
            assert!(matches!(cancel.payload(), AppRequestPayload::CancelArtifact(cancel)
                if cancel.transfer_id() == upload.metadata().transfer_id()
                && cancel.artifact_id() == upload.metadata().artifact_id()));
            assert!(ack(&mut model, &cancel).is_empty());
            assert!(ack(&mut model, &sent).is_empty(), "cancelled upload continued");
        } else {
            assert!(effects.is_empty());
            assert!(
                model
                    .update(Action::FileRead { operation: *operation, result: Ok(file()) })
                    .is_empty()
            );
        }
        assert!(model.chat.workbench.files.import_preview.is_none());
        assert_eq!(model.chat.workbench.files.caption, "Keep this instruction");
        assert_eq!(model.chat.buffer, "Retain this new draft");
        assert!(model.chat.workbench.unresolved.is_none());
    }
}
