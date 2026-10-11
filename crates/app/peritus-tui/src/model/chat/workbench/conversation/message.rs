//! Chunked immutable user-message admission shares the scoped text artifact contract.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchIntent};
use crate::image_import::UploadStep;
use peritus_app_protocol::{
    ArtifactChunk, ArtifactCompletion, ArtifactMetadata, CanonicalMediaType, TransferId,
    WellKnownProtocolFeature, WorkbenchFileImportPreview, WorkbenchFileImportRequest,
    WorkbenchFileMetadata, WorkbenchFileMode, WorkbenchFileRange, WorkbenchFileRequest,
    WorkbenchFileUpload, WorkbenchSnapshot,
};
use peritus_types::ArtifactId;

#[derive(Debug)]
pub(super) struct MessageUpload {
    pub(super) request: WorkbenchFileImportRequest,
    metadata: ArtifactMetadata,
    awaiting: UploadStep,
}

impl AppModel {
    pub(super) fn referenced_messages_available(&self) -> bool {
        [
            WellKnownProtocolFeature::WorkbenchFiles,
            WellKnownProtocolFeature::WorkbenchReferencedText,
        ]
        .into_iter()
        .all(|required| self.features.iter().any(|feature| feature.as_str() == required.as_str()))
    }

    pub(super) fn begin_message_upload(&mut self, snapshot: &WorkbenchSnapshot) -> Vec<Effect> {
        let Some(submission) = self.chat.workbench.submission.as_ref() else { return Vec::new() };
        let Some(text) = submission.message.as_ref() else { return Vec::new() };
        if !self.referenced_messages_available() || submission.stopped {
            return Vec::new();
        }
        let setup = (|| {
            let digest = peritus_codec::sha256(text.as_bytes());
            let length = u64::try_from(text.len()).ok()?;
            let artifact = ArtifactId::new(self.ids.bytes(b"message-artifact")).ok()?;
            let transfer = TransferId::new(self.ids.bytes(b"message-upload")).ok()?;
            let preferred = self.limits.max_artifact_chunk_bytes().min(64 * 1024);
            let metadata = ArtifactMetadata::new(
                transfer,
                artifact,
                length,
                CanonicalMediaType::new("text/plain".to_owned(), 128).ok()?,
                digest,
                u32::try_from(preferred).ok()?,
                self.limits.max_artifact_chunk_bytes(),
            )
            .ok()?;
            let selection = WorkbenchFileRequest::new(
                snapshot.query(),
                snapshot.revision(),
                "User message".to_owned(),
                WorkbenchFileRange::All,
                WorkbenchFileMode::Snapshot,
                submission.settings.providers().writer(),
                submission.settings.models().writer().clone(),
            )
            .ok()?;
            let file = WorkbenchFileMetadata::new(digest, length, (0, length), digest).ok()?;
            let request = WorkbenchFileImportRequest::new(selection, artifact, file).ok()?;
            let upload =
                WorkbenchFileUpload::new(snapshot.query(), snapshot.revision(), metadata.clone())
                    .ok()?;
            Some((metadata, request, upload))
        })();
        let Some((metadata, request, upload)) = setup else {
            self.notice(
                NoticeLevel::Warning,
                "Unable to prepare exact message transfer; draft retained.",
            );
            return Vec::new();
        };
        let effect = self.request(
            AppRequestPayload::BeginWorkbenchFileUpload(upload),
            PendingRequest::WorkbenchMessageUpload {
                transfer: metadata.transfer_id(),
                step: UploadStep::Begin,
            },
        );
        if effect.is_some()
            && let Some(submission) = self.chat.workbench.submission.as_mut()
        {
            submission.upload =
                Some(MessageUpload { metadata, request, awaiting: UploadStep::Begin });
        }
        effect.into_iter().collect()
    }

    pub(in crate::model) fn message_upload_ack(
        &mut self,
        transfer: TransferId,
        step: UploadStep,
    ) -> Vec<Effect> {
        let Some(submission) = self.chat.workbench.submission.as_ref() else { return Vec::new() };
        let Some(upload) = submission
            .upload
            .as_ref()
            .filter(|upload| upload.metadata.transfer_id() == transfer && upload.awaiting == step)
        else {
            return Vec::new();
        };
        if submission.stopped {
            self.chat.workbench.submission = None;
            self.notice(
                NoticeLevel::Info,
                "Message upload stopped before queue admission; draft retained.",
            );
            return Vec::new();
        }
        if step == UploadStep::Complete {
            let request = upload.request.clone();
            return self
                .request(
                    AppRequestPayload::PreviewWorkbenchFileImport(request.clone()),
                    PendingRequest::WorkbenchMessagePreview(request),
                )
                .into_iter()
                .collect();
        }
        let Some(text) = submission.message.as_ref() else { return Vec::new() };
        let offset = match step {
            UploadStep::Chunk { end } => end,
            UploadStep::Begin | UploadStep::Complete => 0,
        };
        let (payload, next) = if offset == text.len() {
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
            let end = offset.saturating_add(length).min(text.len());
            let Ok(chunk) = ArtifactChunk::new(
                transfer,
                upload.metadata.artifact_id(),
                (offset / length) as u64,
                offset as u64,
                text.as_bytes()[offset..end].to_vec(),
                self.limits.max_artifact_chunk_bytes(),
            ) else {
                self.message_upload_failed();
                return Vec::new();
            };
            (AppRequestPayload::UploadArtifactChunk(chunk), UploadStep::Chunk { end })
        };
        let effect =
            self.request(payload, PendingRequest::WorkbenchMessageUpload { transfer, step: next });
        if effect.is_some()
            && let Some(upload) = self
                .chat
                .workbench
                .submission
                .as_mut()
                .and_then(|submission| submission.upload.as_mut())
        {
            upload.awaiting = next;
        }
        effect.into_iter().collect()
    }

    pub(in crate::model) fn accept_message_preview(
        &mut self,
        request: &WorkbenchFileImportRequest,
        preview: WorkbenchFileImportPreview,
    ) -> Vec<Effect> {
        let Some(submission) = self.chat.workbench.submission.as_ref() else { return Vec::new() };
        if submission.stopped
            || preview.request() != request
            || submission.upload.as_ref().is_none_or(|upload| &upload.request != request)
            || self.chat.workbench.selected != Some(request.selection().query())
            || !self.referenced_messages_available()
        {
            self.message_upload_failed();
            return Vec::new();
        }
        // Enter already explicitly authorized this exact message. This is message admission,
        // not the inert file-picker preview flow; the host still verifies the complete consent.
        self.submit_workbench_chat_intent(
            WorkbenchIntent::EnqueueMessage { preview },
            request.selection().query(),
            request.selection().revision(),
        )
    }

    pub(in crate::model) fn message_upload_failed(&mut self) {
        if self
            .chat
            .workbench
            .submission
            .as_ref()
            .is_some_and(|submission| !submission.queued && submission.message.is_some())
        {
            self.chat.workbench.submission = None;
            self.notice(
                NoticeLevel::Warning,
                "Message staging failed; draft retained. Send again to retry admission.",
            );
        }
    }
}
