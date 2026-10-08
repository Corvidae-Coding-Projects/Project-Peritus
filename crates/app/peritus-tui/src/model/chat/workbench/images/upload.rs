//! Acknowledgement-driven upload; only completion requests the daemon's decoded preview.

use super::{
    AppModel, AppRequestPayload, ArtifactMetadata, ControlOperationId, Effect, ImageBytes,
    ImageUploadStep, NoticeLevel, PendingRequest, Upload, WorkbenchImageRequest,
};
use peritus_app_protocol::{
    ArtifactChunk, ArtifactCompletion, CanonicalMediaType, TransferId, WorkbenchImageUpload,
};
use peritus_types::ArtifactId;

impl AppModel {
    pub(in crate::model) fn image_read_complete(
        &mut self,
        operation: ControlOperationId,
        result: Result<ImageBytes, &'static str>,
    ) -> Vec<Effect> {
        if self
            .chat
            .workbench
            .images
            .reading
            .as_ref()
            .is_none_or(|binding| binding.operation != operation)
        {
            return Vec::new();
        }
        let Some(binding) = self.chat.workbench.images.reading.take() else {
            return Vec::new();
        };
        if !self.images_available() || self.chat.workbench.selected != Some(binding.query) {
            return Vec::new();
        }
        let image = match result {
            Ok(image) => image,
            Err(error) => {
                self.chat.workbench.message = format!("{error} Draft retained; nothing included.");
                return Vec::new();
            }
        };
        let setup = (|| {
            let artifact = ArtifactId::new(self.ids.bytes(b"image-artifact")).ok()?;
            let transfer = TransferId::new(self.ids.bytes(b"image-upload")).ok()?;
            let preferred = self.limits.max_artifact_chunk_bytes().min(64 * 1024);
            let metadata = ArtifactMetadata::new(
                transfer,
                artifact,
                image.byte_len,
                CanonicalMediaType::new("application/octet-stream".to_owned(), 128).ok()?,
                image.digest,
                u32::try_from(preferred).ok()?,
                self.limits.max_artifact_chunk_bytes(),
            )
            .ok()?;
            let request = WorkbenchImageRequest::new(
                binding.query,
                binding.revision,
                artifact,
                binding.provider,
                binding.model,
                image.label.clone(),
            )
            .ok()?;
            let upload =
                WorkbenchImageUpload::new(binding.query, binding.revision, metadata.clone())
                    .ok()?;
            Some((metadata, request, upload))
        })();
        let Some((metadata, request, upload)) = setup else {
            self.notice(
                NoticeLevel::Error,
                "Selected file exceeds negotiated transfer limits; draft retained.",
            );
            return Vec::new();
        };
        let Some(effect) = self.request(
            AppRequestPayload::BeginWorkbenchImageUpload(upload),
            PendingRequest::WorkbenchImageUpload {
                transfer: metadata.transfer_id(),
                step: ImageUploadStep::Begin,
            },
        ) else {
            return Vec::new();
        };
        self.chat.workbench.images.expected = Some((image.digest, image.byte_len));
        self.chat.workbench.images.request = Some(request);
        self.chat.workbench.images.upload =
            Some(Upload { image, metadata, awaiting: ImageUploadStep::Begin });
        "Uploading exact bytes to the local daemon for validation; not to a provider."
            .clone_into(&mut self.chat.workbench.message);
        vec![effect]
    }

    pub(in crate::model) fn image_upload_ack(
        &mut self,
        transfer: TransferId,
        step: ImageUploadStep,
    ) -> Vec<Effect> {
        let matches = self
            .chat
            .workbench
            .images
            .upload
            .as_ref()
            .is_some_and(|upload| {
                upload.metadata.transfer_id() == transfer && upload.awaiting == step
            });
        if !matches {
            return Vec::new();
        }
        if step == ImageUploadStep::Complete {
            self.chat.workbench.images.upload = None;
            let Some(request) = self.chat.workbench.images.request.clone() else {
                return Vec::new();
            };
            "Decoding with host/provider limits; waiting for an exact preview. No inference."
                .clone_into(&mut self.chat.workbench.message);
            return self
                .request(
                    AppRequestPayload::PreviewWorkbenchImage(request.clone()),
                    PendingRequest::WorkbenchImagePreview(request),
                )
                .into_iter()
                .collect();
        }
        let offset = match step {
            ImageUploadStep::Chunk { end } => end,
            ImageUploadStep::Begin | ImageUploadStep::Complete => 0,
        };
        let maximum_chunk_bytes = self.limits.max_artifact_chunk_bytes();
        let prepared: Result<_, &'static str> = (|| {
            let upload = self
                .chat
                .workbench
                .images
                .upload
                .as_mut()
                .expect("matched image upload");
            if offset == upload.image.byte_len {
                Ok((
                    AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
                        transfer,
                        upload.metadata.artifact_id(),
                        upload.metadata.byte_size(),
                        upload.metadata.digest(),
                    )),
                    ImageUploadStep::Complete,
                ))
            } else {
                let length = upload.metadata.preferred_chunk_size() as usize;
                let bytes = upload.image.read_chunk(offset, length)?;
                let byte_count =
                    u64::try_from(bytes.len()).map_err(|_| "Image chunk is too large.")?;
                let end =
                    offset.checked_add(byte_count).ok_or("Image upload offset overflow.")?;
                let chunk = ArtifactChunk::new(
                    transfer,
                    upload.metadata.artifact_id(),
                    offset / u64::from(upload.metadata.preferred_chunk_size()),
                    offset,
                    bytes,
                    maximum_chunk_bytes,
                )
                .map_err(|_| "Image upload chunk is invalid.")?;
                Ok((
                    AppRequestPayload::UploadArtifactChunk(chunk),
                    ImageUploadStep::Chunk { end },
                ))
            }
        })();
        let Ok((payload, next)) = prepared else {
            self.interrupt_image_import();
            return Vec::new();
        };
        let effect =
            self.request(payload, PendingRequest::WorkbenchImageUpload { transfer, step: next });
        if let Some(upload) = self.chat.workbench.images.upload.as_mut() {
            upload.awaiting = next;
        }
        effect.into_iter().collect()
    }

    pub(in crate::model) fn image_request_error(
        &mut self,
        pending: Option<&PendingRequest>,
        code: peritus_app_protocol::AppErrorCode,
    ) {
        let matched = match pending {
            Some(PendingRequest::WorkbenchImageUpload { transfer, .. }) => self
                .chat
                .workbench
                .images
                .upload
                .as_ref()
                .is_some_and(|upload| upload.metadata.transfer_id() == *transfer),
            Some(PendingRequest::WorkbenchImagePreview(request)) => {
                self.chat.workbench.images.request.as_ref() == Some(request)
            }
            _ => false,
        };
        if matched {
            self.chat.workbench.images.upload = None;
            self.chat.workbench.images.preview = None;
            let explanation = match code {
                peritus_app_protocol::AppErrorCode::MissingRequiredFeature => {
                    "Selected provider/model/effort is unavailable or lacks image input; choose a compatible provider."
                }
                peritus_app_protocol::AppErrorCode::MalformedFrame => {
                    "Invalid image/MIME or decoding exceeds the bounded physical working set: PNG/JPEG/GIF/WebP."
                }
                peritus_app_protocol::AppErrorCode::LimitExceeded => {
                    "Image exceeds the selected provider or configured artifact-storage limit; nothing was truncated."
                }
                peritus_app_protocol::AppErrorCode::StaleRevision => {
                    "Conversation or provider changed; refresh before previewing again."
                }
                _ => "Refresh and explicitly preview again after resolving this error.",
            };
            self.chat.workbench.message = format!(
                "Import rejected: {}. {explanation} No image included; draft retained.",
                code.as_str()
            );
        }
    }
}
