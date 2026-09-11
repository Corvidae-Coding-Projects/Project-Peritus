//! Acknowledgement-driven external-text upload and immutable preview confirmation.

use super::{
    AppModel, AppRequestPayload, ArtifactMetadata, ControlOperationId, Effect, FileBytes,
    NoticeLevel, PendingRequest, ReadBinding, Upload, UploadStep, WorkbenchFileImportPreview,
    WorkbenchFileImportRequest, WorkbenchFileMode, WorkbenchFileRange, WorkbenchFileRequest,
    WorkbenchInputText, WorkbenchIntent,
};
use peritus_app_protocol::{
    ArtifactChunk, ArtifactCompletion, CanonicalMediaType, TransferId, WorkbenchFileUpload,
};
use peritus_types::ArtifactId;

impl AppModel {
    pub(super) fn begin_file_import(&mut self, range: WorkbenchFileRange) -> Vec<Effect> {
        if !self.files_available()
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
        {
            return Vec::new();
        }
        let Some(snapshot) = self
            .chat
            .workbench
            .snapshot
            .as_ref()
            .filter(|snapshot| Some(snapshot.query()) == self.chat.workbench.selected)
        else {
            self.notice(
                NoticeLevel::Warning,
                "Refresh the conversation before importing; draft retained.",
            );
            return Vec::new();
        };
        let Some(provider) = self
            .product
            .as_ref()
            .filter(|product| product.launch.workspace_id() == snapshot.query().workspace())
            .and_then(crate::model::product::ProductUi::providers)
            .map(peritus_app_protocol::ProductProviderSelection::writer)
        else {
            self.notice(
                NoticeLevel::Warning,
                "Choose a writer provider before importing external text.",
            );
            return Vec::new();
        };
        let Ok(operation) = ControlOperationId::new(self.ids.bytes(b"explicit-file-read")) else {
            return Vec::new();
        };
        let file = &mut self.chat.workbench.files;
        if file.refresh {
            file.refresh = false;
        }
        "Reading only the selected external file as an immutable snapshot. Nothing is sent to a provider."
            .clone_into(&mut self.chat.workbench.message);
        file.reading = Some(ReadBinding {
            operation,
            query: snapshot.query(),
            revision: snapshot.revision(),
            provider,
            model: self.chat.models.writer().clone(),
            range,
            path: file.path.clone(),
        });
        file.discard_preview();
        file.editing = None;
        vec![Effect::ReadFile { operation, path: file.path.clone().into(), range }]
    }

    pub(in crate::model) fn file_read_complete(
        &mut self,
        operation: ControlOperationId,
        result: Result<FileBytes, &'static str>,
    ) -> Vec<Effect> {
        if self
            .chat
            .workbench
            .files
            .reading
            .as_ref()
            .is_none_or(|binding| binding.operation != operation)
        {
            return Vec::new();
        }
        let Some(binding) = self.chat.workbench.files.reading.take() else {
            return Vec::new();
        };
        if !self.files_available()
            || self.chat.workbench.selected != Some(binding.query)
            || self.chat.workbench.files.path != binding.path
        {
            return Vec::new();
        }
        let file = match result {
            Ok(file) => file,
            Err(error) => {
                self.chat.workbench.message = format!("{error} Draft retained; nothing included.");
                return Vec::new();
            }
        };
        let setup = (|| {
            let artifact = ArtifactId::new(self.ids.bytes(b"file-import-artifact")).ok()?;
            let transfer = TransferId::new(self.ids.bytes(b"file-import-upload")).ok()?;
            let preferred = self.limits.max_artifact_chunk_bytes().min(64 * 1024);
            let metadata = ArtifactMetadata::new(
                transfer,
                artifact,
                file.bytes.len() as u64,
                CanonicalMediaType::new("text/plain".to_owned(), 128).ok()?,
                file.file.digest(),
                u32::try_from(preferred).ok()?,
                self.limits.max_artifact_chunk_bytes(),
            )
            .ok()?;
            let selection = WorkbenchFileRequest::new(
                binding.query,
                binding.revision,
                file.label.clone(),
                binding.range,
                WorkbenchFileMode::Snapshot,
                binding.provider,
                binding.model,
            )
            .ok()?;
            let request = WorkbenchFileImportRequest::new(selection, artifact, file.file).ok()?;
            let upload =
                WorkbenchFileUpload::new(binding.query, binding.revision, metadata.clone()).ok()?;
            Some((metadata, request, upload))
        })();
        let Some((metadata, request, upload)) = setup else {
            self.notice(
                NoticeLevel::Error,
                "Selected text exceeds negotiated transfer limits; draft retained.",
            );
            return Vec::new();
        };
        let Some(effect) = self.request(
            AppRequestPayload::BeginWorkbenchFileUpload(upload),
            PendingRequest::WorkbenchFileUpload {
                transfer: metadata.transfer_id(),
                step: UploadStep::Begin,
            },
        ) else {
            return Vec::new();
        };
        let path = binding.path;
        self.chat.workbench.files.expected = Some((file.file, path));
        self.chat.workbench.files.import_request = Some(request);
        self.chat.workbench.files.upload =
            Some(Upload { file, metadata, awaiting: UploadStep::Begin });
        "Uploading selected text to the local daemon for validation; not to a provider."
            .clone_into(&mut self.chat.workbench.message);
        vec![effect]
    }

    pub(in crate::model) fn file_upload_ack(
        &mut self,
        transfer: TransferId,
        step: UploadStep,
    ) -> Vec<Effect> {
        let Some(upload) =
            self.chat.workbench.files.upload.as_ref().filter(|upload| {
                upload.metadata.transfer_id() == transfer && upload.awaiting == step
            })
        else {
            return Vec::new();
        };
        if step == UploadStep::Complete {
            self.chat.workbench.files.upload = None;
            let Some(request) = self.chat.workbench.files.import_request.clone() else {
                return Vec::new();
            };
            "Validating exact UTF-8 bytes and provider binding; no inference."
                .clone_into(&mut self.chat.workbench.message);
            return self
                .request(
                    AppRequestPayload::PreviewWorkbenchFileImport(request.clone()),
                    PendingRequest::WorkbenchFileImportPreview(request),
                )
                .into_iter()
                .collect();
        }
        let offset = match step {
            UploadStep::Chunk { end } => end,
            UploadStep::Begin | UploadStep::Complete => 0,
        };
        let (payload, next) = if offset == upload.file.bytes.len() {
            (
                AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
                    transfer,
                    upload.metadata.artifact_id(),
                    upload.metadata.byte_size(),
                    upload.metadata.digest(),
                )),
                UploadStep::Complete,
            )
        } else {
            let length = upload.metadata.preferred_chunk_size() as usize;
            let end = offset.saturating_add(length).min(upload.file.bytes.len());
            let Ok(chunk) = ArtifactChunk::new(
                transfer,
                upload.metadata.artifact_id(),
                (offset / length) as u64,
                offset as u64,
                upload.file.bytes[offset..end].to_vec(),
                self.limits.max_artifact_chunk_bytes(),
            ) else {
                self.interrupt_file_import();
                return Vec::new();
            };
            (AppRequestPayload::UploadArtifactChunk(chunk), UploadStep::Chunk { end })
        };
        let effect =
            self.request(payload, PendingRequest::WorkbenchFileUpload { transfer, step: next });
        if let Some(upload) = self.chat.workbench.files.upload.as_mut() {
            upload.awaiting = next;
        }
        effect.into_iter().collect()
    }

    pub(in crate::model) fn accept_file_import_preview(
        &mut self,
        request: &WorkbenchFileImportRequest,
        preview: WorkbenchFileImportPreview,
    ) {
        let file = &mut self.chat.workbench.files;
        if file.import_request.as_ref() != Some(request)
            || preview.request() != request
            || self.chat.workbench.selected != Some(request.selection().query())
            || file
                .expected
                .as_ref()
                .is_none_or(|(metadata, path)| *metadata != request.file() || *path != file.path)
        {
            self.notice(
                NoticeLevel::Error,
                "File import preview did not match the exact local read and selection; no confirmation is available.",
            );
            return;
        }
        file.preview = None;
        file.import_preview = Some(preview);
        self.chat.workbench.scroll = 0;
        "Preview only. Review source/range/digests, then press c to confirm immutable text."
            .clone_into(&mut self.chat.workbench.message);
    }

    pub(super) fn confirm_file_import(&mut self) -> Vec<Effect> {
        let Some(preview) = self.chat.workbench.files.import_preview.clone() else {
            return Vec::new();
        };
        let provider = self
            .product
            .as_ref()
            .and_then(crate::model::product::ProductUi::providers)
            .map(peritus_app_protocol::ProductProviderSelection::writer);
        if provider != Some(preview.request().selection().provider())
            || self.chat.models.writer() != preview.request().selection().model()
        {
            self.notice(
                NoticeLevel::Warning,
                "Provider/model changed after preview. Preview again before confirmation.",
            );
            return Vec::new();
        }
        let Ok(text) = WorkbenchInputText::new(self.chat.workbench.files.caption.clone()) else {
            self.notice(
                NoticeLevel::Warning,
                "Enter an explicit caption/instruction with t before confirming.",
            );
            return Vec::new();
        };
        let selection = preview.request().selection();
        self.send_file_intent(
            selection.query(),
            selection.revision(),
            WorkbenchIntent::AttachFileImport { preview, text },
        )
    }

    pub(in crate::model) fn file_read_failed(&mut self) {
        if self.chat.workbench.files.reading.take().is_some() {
            "External file read failed; draft retained and nothing included."
                .clone_into(&mut self.chat.workbench.message);
        }
    }

    pub(in crate::model) fn interrupt_file_import(&mut self) {
        let file = &mut self.chat.workbench.files;
        let reading = file.reading.take().is_some();
        let uploading = file.upload.take().is_some();
        if reading || uploading || (file.import_request.is_some() && file.import_preview.is_none())
        {
            file.discard_preview();
            "Import interrupted. Draft retained; explicitly preview again. No text acceptance was inferred."
                .clone_into(&mut self.chat.workbench.message);
        }
    }
}
