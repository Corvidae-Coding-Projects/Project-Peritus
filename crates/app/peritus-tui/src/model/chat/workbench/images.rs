//! Explicit import drafts and preview-bound confirmation. No input is included before a receipt.

use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchIntent,
    WorkbenchMode,
};
use crate::image_import::{ImageBytes, UploadStep};
use peritus_app_protocol::{
    ArtifactMetadata, ControlOperationId, ProductModelChoice, WellKnownProtocolFeature,
    WorkbenchImagePreview, WorkbenchImageRequest, WorkbenchInputText, WorkbenchQuery,
};
use peritus_types::{ProviderProfileId, Sha256Digest};

mod editor;
mod page;
mod upload;

#[derive(Debug, Default)]
pub struct ImageUi {
    pub(crate) open: bool,
    pub(crate) path: String,
    pub(crate) caption: String,
    pub(crate) editing: Option<ImageField>,
    pub(crate) cursor: usize,
    pub(crate) preview: Option<WorkbenchImagePreview>,
    pub(crate) list: bool,
    pub(crate) page: Option<peritus_app_protocol::WorkbenchImagePage>,
    pub(crate) selected: usize,
    pub(super) reading: Option<ReadBinding>,
    upload: Option<Upload>,
    expected: Option<(Sha256Digest, u64)>,
    request: Option<WorkbenchImageRequest>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageField {
    Path,
    Caption,
}
impl ImageUi {
    pub(super) fn discard_preview(&mut self) {
        self.preview = None;
        self.request = None;
        self.expected = None;
    }
    pub(crate) fn editor(&self) -> Option<(&'static str, &str, usize)> {
        self.editing.map(|field| match field {
            ImageField::Path => ("Absolute image path", self.path.as_str(), self.cursor),
            ImageField::Caption => ("Caption / instruction", self.caption.as_str(), self.cursor),
        })
    }
}
#[derive(Debug)]
pub(super) struct ReadBinding {
    operation: ControlOperationId,
    query: WorkbenchQuery,
    revision: u64,
    provider: ProviderProfileId,
    model: ProductModelChoice,
}
#[derive(Debug)]
struct Upload {
    image: ImageBytes,
    metadata: ArtifactMetadata,
    awaiting: UploadStep,
}

impl AppModel {
    pub(in crate::model::chat) fn image_command(&mut self, path: &str) -> Vec<Effect> {
        if !self.images_available() {
            self.notice(NoticeLevel::Warning, "Image import unavailable/offline; reconnect or upgrade the daemon. Draft retained.");
            return Vec::new();
        }
        if self.chat.workbench.selected.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation with /sessions first. Draft retained.",
            );
            return Vec::new();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending request before another import. Draft retained.",
            );
            return Vec::new();
        }
        self.chat.workbench.open = true;
        self.chat.workbench.images.open = true;
        self.chat.workbench.mode = WorkbenchMode::Sessions;
        self.chat.workbench.context_mode = None;
        if path.is_empty() {
            self.chat.workbench.images.list = true;
            return self.refresh_workbench();
        }
        if path.len() > 4096 || path.chars().any(char::is_control) {
            self.notice(
                NoticeLevel::Warning,
                "Path exceeds 4096 bytes or contains control characters; draft retained.",
            );
            return Vec::new();
        }
        path.clone_into(&mut self.chat.workbench.images.path);
        self.chat.workbench.images.list = false;
        self.begin_image_read()
    }

    fn images_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchImages.as_str()
            })
    }
    fn begin_image_read(&mut self) -> Vec<Effect> {
        if !self.images_available()
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
                "Choose a chat/writer provider before importing an image.",
            );
            return Vec::new();
        };
        let Ok(operation) = ControlOperationId::new(self.ids.bytes(b"explicit-image-read")) else {
            return Vec::new();
        };
        let image = &mut self.chat.workbench.images;
        image.reading = Some(ReadBinding {
            operation,
            query: snapshot.query(),
            revision: snapshot.revision(),
            provider,
            model: self.chat.models.writer().clone(),
        });
        image.preview = None;
        image.request = None;
        image.expected = None;
        image.editing = None;
        "Reading only the selected file. Nothing has been sent to a provider."
            .clone_into(&mut self.chat.workbench.message);
        vec![Effect::ReadImage { operation, path: image.path.clone().into() }]
    }

    pub(in crate::model) fn accept_image_preview(
        &mut self,
        request: &WorkbenchImageRequest,
        preview: WorkbenchImagePreview,
    ) {
        let image = &mut self.chat.workbench.images;
        if image.request.as_ref() != Some(request)
            || preview.request() != request
            || self.chat.workbench.selected != Some(request.query())
            || image.expected != Some((preview.image().digest(), preview.image().bytes()))
        {
            self.notice(NoticeLevel::Error, "Image preview did not match the exact imported bytes and selection; no confirmation is available.");
            return;
        }
        image.preview = Some(preview);
        self.chat.workbench.scroll = 0;
        "Preview only: no provider request, no inclusion yet. Review the metadata and caption, then press c to confirm.".clone_into(&mut self.chat.workbench.message);
    }

    fn confirm_image(&mut self) -> Vec<Effect> {
        if !self.images_available()
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
        {
            return Vec::new();
        }
        let Some(preview) = self.chat.workbench.images.preview.clone() else {
            self.notice(NoticeLevel::Warning, "Read and preview an image before confirming.");
            return Vec::new();
        };
        let selected = self
            .product
            .as_ref()
            .and_then(crate::model::product::ProductUi::providers)
            .map(peritus_app_protocol::ProductProviderSelection::writer);
        if selected != Some(preview.request().provider())
            || self.chat.models.writer() != preview.request().model()
        {
            self.notice(
                NoticeLevel::Warning,
                "Provider/model changed after preview. Preview again before confirmation.",
            );
            return Vec::new();
        }
        let Ok(text) = WorkbenchInputText::new(self.chat.workbench.images.caption.clone()) else {
            self.notice(
                NoticeLevel::Warning,
                "Press t and enter a caption/instruction before confirming; nothing included yet.",
            );
            return Vec::new();
        };
        let workspace = preview.request().query().workspace();
        self.submit_workbench(WorkbenchIntent::AttachImage { preview, text }, workspace)
    }

    pub(in crate::model) fn image_read_failed(&mut self) {
        if self.chat.workbench.images.reading.take().is_some() {
            "Local file read failed; draft retained and nothing included."
                .clone_into(&mut self.chat.workbench.message);
        }
    }
    pub(in crate::model) fn interrupt_image_import(&mut self) {
        let image = &mut self.chat.workbench.images;
        let reading = image.reading.take().is_some();
        let uploading = image.upload.take().is_some();
        if reading || uploading || (image.request.is_some() && image.preview.is_none()) {
            image.request = None;
            image.expected = None;
            "Import interrupted. Draft retained; refresh and explicitly preview again. No image acceptance was inferred.".clone_into(&mut self.chat.workbench.message);
        }
    }
}
