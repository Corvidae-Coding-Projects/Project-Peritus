//! Deliberate file preview/confirmation and retained selection, never automatic path discovery.
use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchCommand,
    WorkbenchIntent, WorkbenchMode,
};
use crate::{file_import::FileBytes, image_import::UploadStep};
use peritus_app_protocol::{
    ArtifactMetadata, ControlOperationId, ProductModelChoice, WellKnownProtocolFeature,
    WorkbenchFileImportPreview, WorkbenchFileImportRequest, WorkbenchFileMetadata,
    WorkbenchFileMode, WorkbenchFilePage, WorkbenchFilePreview, WorkbenchFileQuery,
    WorkbenchFileRange, WorkbenchFileRequest, WorkbenchInputText, WorkbenchQuery,
};
use peritus_types::ProviderProfileId;
mod import;
mod keys;

#[derive(Debug, Default)]
pub struct FileUi {
    pub(crate) open: bool,
    pub(crate) list: bool,
    pub(crate) path: String,
    pub(crate) range: String,
    pub(crate) caption: String,
    pub(crate) refresh: bool,
    pub(crate) preview: Option<WorkbenchFilePreview>,
    pub(crate) import_preview: Option<WorkbenchFileImportPreview>,
    pub(crate) page: Option<WorkbenchFilePage>,
    pub(crate) selected: usize,
    pub(crate) editing: Option<u8>,
    pub(crate) cursor: usize,
    pub(super) reading: Option<ReadBinding>,
    upload: Option<Upload>,
    expected: Option<(WorkbenchFileMetadata, String)>,
    import_request: Option<WorkbenchFileImportRequest>,
}

impl FileUi {
    pub(super) fn discard_preview(&mut self) {
        self.preview = None;
        self.import_preview = None;
        self.expected = None;
        self.import_request = None;
    }
}

#[derive(Debug)]
pub(super) struct ReadBinding {
    operation: ControlOperationId,
    query: WorkbenchQuery,
    revision: u64,
    provider: ProviderProfileId,
    model: ProductModelChoice,
    range: WorkbenchFileRange,
    path: String,
}

#[derive(Debug)]
struct Upload {
    file: FileBytes,
    metadata: ArtifactMetadata,
    awaiting: UploadStep,
}
impl AppModel {
    fn files_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchFiles.as_str()
            })
    }
    pub(in crate::model::chat) fn file_command(&mut self, path: &str) -> Vec<Effect> {
        if !self.files_available() {
            self.notice(
                NoticeLevel::Warning,
                "File controls unavailable; reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        if path.len() > 4096 || path.chars().any(char::is_control) {
            self.notice(
                NoticeLevel::Warning,
                "File path is oversized or contains controls; draft unchanged.",
            );
            return Vec::new();
        }
        if self.chat.workbench.selected.is_none()
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
        {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation and resolve pending receipts before attaching a file.",
            );
            return Vec::new();
        }
        self.chat.workbench.open = true;
        self.chat.workbench.files.open = true;
        self.chat.workbench.images.open = false;
        self.chat.workbench.mode = WorkbenchMode::Sessions;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.files.list = path.is_empty();
        if !path.is_empty() {
            path.clone_into(&mut self.chat.workbench.files.path);
            self.chat.workbench.files.discard_preview();
        }
        self.refresh_file_panel()
    }
    pub(in crate::model) fn refresh_file_panel(&mut self) -> Vec<Effect> {
        if !self.files_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else {
            return Vec::new();
        };
        let snapshot =
            self.chat.workbench.snapshot.as_ref().filter(|snapshot| snapshot.query() == query);
        if let Some(snapshot) = snapshot {
            if self.chat.workbench.files.list {
                return self.file_page(snapshot.revision(), 0);
            }
            return Vec::new();
        }
        self.request(
            AppRequestPayload::QueryWorkbench(query),
            PendingRequest::WorkbenchQuery(query),
        )
        .into_iter()
        .collect()
    }
    fn file_page(&mut self, revision: u64, offset: u32) -> Vec<Effect> {
        let Some(query) = self.chat.workbench.selected else {
            return Vec::new();
        };
        let Ok(query) = WorkbenchFileQuery::new(query, revision, offset) else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryWorkbenchFiles(query),
            PendingRequest::WorkbenchFiles(query),
        )
        .into_iter()
        .collect()
    }
    fn preview_file(&mut self) -> Vec<Effect> {
        if !self.files_available() {
            return Vec::new();
        }
        let Some(snapshot) = &self.chat.workbench.snapshot else {
            return self.refresh_file_panel();
        };
        let Some(provider) = self
            .product
            .as_ref()
            .filter(|product| product.launch.workspace_id() == snapshot.query().workspace())
            .and_then(crate::model::product::ProductUi::providers)
            .map(peritus_app_protocol::ProductProviderSelection::writer)
        else {
            self.notice(NoticeLevel::Warning, "Choose a writer provider before previewing.");
            return Vec::new();
        };
        let file = &self.chat.workbench.files;
        let Some(range) = parse_range(&file.range) else {
            self.notice(
                NoticeLevel::Warning,
                "Range: all, lines:1:20, or bytes:0:1024. Nothing read.",
            );
            return Vec::new();
        };
        if std::path::Path::new(&file.path).is_absolute() {
            return self.begin_file_import(range);
        }
        let mode = if file.refresh {
            WorkbenchFileMode::RefreshOnRequest
        } else {
            WorkbenchFileMode::Snapshot
        };
        let Ok(request) = WorkbenchFileRequest::new(
            snapshot.query(),
            snapshot.revision(),
            file.path.clone(),
            range,
            mode,
            provider,
            self.chat.models.writer().clone(),
        ) else {
            return Vec::new();
        };
        self.chat.workbench.files.discard_preview();
        self.request(
            AppRequestPayload::PreviewWorkbenchFile(request.clone()),
            PendingRequest::WorkbenchFilePreview(request),
        )
        .into_iter()
        .collect()
    }
    pub(in crate::model) fn accept_file_preview(
        &mut self,
        request: &WorkbenchFileRequest,
        preview: WorkbenchFilePreview,
    ) {
        if preview.request() == request
            && self.chat.workbench.selected == Some(request.query())
            && self.chat.workbench.files.path == request.path()
        {
            self.chat.workbench.files.import_preview = None;
            self.chat.workbench.files.preview = Some(preview);
            self.chat.workbench.scroll = 0;
            "Preview only. Press c to confirm exact bytes; no inference starts."
                .clone_into(&mut self.chat.workbench.message);
        }
    }
    pub(in crate::model) fn accept_file_page(
        &mut self,
        query: WorkbenchFileQuery,
        page: WorkbenchFilePage,
    ) {
        if page.query() == query && self.chat.workbench.selected == Some(query.query()) {
            self.chat.workbench.files.page = Some(page);
            self.chat.workbench.files.selected = 0;
            self.chat.workbench.scroll = 0;
        }
    }
    fn confirm_file(&mut self) -> Vec<Effect> {
        if self.chat.workbench.files.import_preview.is_some() {
            return self.confirm_file_import();
        }
        let Some(preview) = self.chat.workbench.files.preview.clone() else {
            return Vec::new();
        };
        let provider = self
            .product
            .as_ref()
            .and_then(crate::model::product::ProductUi::providers)
            .map(peritus_app_protocol::ProductProviderSelection::writer);
        if provider != Some(preview.request().provider())
            || self.chat.models.writer() != preview.request().model()
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
        let query = preview.request().query();
        let revision = preview.request().revision();
        self.send_file_intent(query, revision, WorkbenchIntent::AttachFile { preview, text })
    }
    fn send_file_intent(
        &mut self,
        query: WorkbenchQuery,
        revision: u64,
        intent: WorkbenchIntent,
    ) -> Vec<Effect> {
        if !self.files_available()
            || self.chat.workbench.selected != Some(query)
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
        {
            return Vec::new();
        }
        let Ok(id) = ControlOperationId::new(self.ids.bytes(b"workbench-file-operation")) else {
            return Vec::new();
        };
        let command = WorkbenchCommand::new(id, query, revision, intent);
        let Some(effect) = self.request(
            AppRequestPayload::WorkbenchCommand(command.clone()),
            PendingRequest::WorkbenchControl(command.clone()),
        ) else {
            return Vec::new();
        };
        self.chat.workbench.unresolved = Some((command, self.chat.buffer.clone()));
        "Awaiting durable file receipt; not yet accepted."
            .clone_into(&mut self.chat.workbench.message);
        vec![effect]
    }
}
fn parse_range(text: &str) -> Option<WorkbenchFileRange> {
    if text.is_empty() || text == "all" {
        return Some(WorkbenchFileRange::All);
    }
    let fields: Vec<_> = text.split(':').collect();
    let range = match fields.as_slice() {
        ["lines", first, last] => {
            WorkbenchFileRange::Lines { first: first.parse().ok()?, last: last.parse().ok()? }
        }
        ["bytes", start, end] => {
            WorkbenchFileRange::Bytes { start: start.parse().ok()?, end: end.parse().ok()? }
        }
        _ => return None,
    };
    range.validate().ok()?;
    Some(range)
}
