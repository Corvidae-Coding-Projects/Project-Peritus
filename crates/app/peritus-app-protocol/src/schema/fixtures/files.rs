//! Exact file consent and retained version compatibility frames.
use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ArtifactMetadata,
    CanonicalMediaType, ControlOperationId, ConversationId, CorrelationId, ProductModelChoice,
    RequestId, TransferId, WorkbenchCommand, WorkbenchFileImportPreview,
    WorkbenchFileImportRequest, WorkbenchFileMetadata, WorkbenchFileMode, WorkbenchFilePage,
    WorkbenchFilePreview, WorkbenchFileQuery, WorkbenchFileRange, WorkbenchFileRequest,
    WorkbenchFileRow, WorkbenchFileUpload, WorkbenchInputText, WorkbenchIntent, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{ArtifactId, ProviderProfileId, Sha256Digest, WorkspaceId};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let scope = WorkbenchQuery::new(id(41, ConversationId::new), id(32, WorkspaceId::new));
    let mut cases = preview_requests(scope, limits)?;
    cases.extend(confirmation_cases(scope, limits)?);
    cases.extend(import_cases(scope, limits)?);
    Ok(cases)
}

fn import_cases(
    scope: WorkbenchQuery,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let artifact = id(67, ArtifactId::new);
    let digest = Sha256Digest::new([64; 32]);
    let metadata = WorkbenchFileMetadata::new(Sha256Digest::new([63; 32]), 20, (6, 14), digest)
        .expect("file metadata");
    let transfer = ArtifactMetadata::new(
        id(68, TransferId::new),
        artifact,
        8,
        CanonicalMediaType::new("text/plain".to_owned(), 128).expect("MIME"),
        digest,
        1024,
        1024,
    )
    .expect("transfer");
    let selection = WorkbenchFileRequest::new(
        scope,
        9,
        "external.txt".to_owned(),
        WorkbenchFileRange::Lines { first: 2, last: 2 },
        WorkbenchFileMode::Snapshot,
        id(61, ProviderProfileId::new),
        ProductModelChoice::default(),
    )
    .expect("selection");
    let request_value =
        WorkbenchFileImportRequest::new(selection, artifact, metadata).expect("request");
    let preview =
        WorkbenchFileImportPreview::new(request_value.clone(), 2, "configured-model".to_owned())
            .expect("preview");
    let command = WorkbenchCommand::new(
        id(69, ControlOperationId::new),
        scope,
        9,
        WorkbenchIntent::AttachFileImport {
            preview: preview.clone(),
            text: WorkbenchInputText::new("Use this immutable import".to_owned()).expect("caption"),
        },
    );
    Ok(vec![
        encoded(
            "minimal-workbench-file-import-upload",
            FixtureClass::Minimal,
            &request(AppRequestPayload::BeginWorkbenchFileUpload(
                WorkbenchFileUpload::new(scope, 9, transfer).expect("upload"),
            )),
            limits,
        )?,
        encoded(
            "realistic-workbench-file-import-preview-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::PreviewWorkbenchFileImport(request_value)),
            limits,
        )?,
        encoded(
            "realistic-workbench-file-import-preview",
            FixtureClass::Realistic,
            &AppResponseEnvelope::new(
                context(),
                id(10, RequestId::new),
                id(11, CorrelationId::new),
                AppResponsePayload::WorkbenchFileImportPreview(preview),
            ),
            limits,
        )?,
        encoded(
            "realistic-workbench-file-import-confirm",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(command.clone())),
            limits,
        )?,
        encoded(
            "realistic-workbench-file-import-receipt-query",
            FixtureClass::Realistic,
            &request(AppRequestPayload::QueryWorkbenchReceipt(command)),
            limits,
        )?,
    ])
}

fn preview_requests(
    scope: WorkbenchQuery,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let mut cases = Vec::new();
    for (name, range, mode) in [
        ("minimal-workbench-file-all", WorkbenchFileRange::All, WorkbenchFileMode::Snapshot),
        (
            "realistic-workbench-file-lines",
            WorkbenchFileRange::Lines { first: 2, last: 4 },
            WorkbenchFileMode::RefreshOnRequest,
        ),
        (
            "realistic-workbench-file-bytes",
            WorkbenchFileRange::Bytes { start: 0, end: 8 },
            WorkbenchFileMode::Snapshot,
        ),
    ] {
        let selection = WorkbenchFileRequest::new(
            scope,
            9,
            "src/reference.txt".to_owned(),
            range,
            mode,
            id(61, ProviderProfileId::new),
            ProductModelChoice::default(),
        )
        .expect("request");
        cases.push(encoded(
            name,
            FixtureClass::Realistic,
            &request(AppRequestPayload::PreviewWorkbenchFile(selection)),
            limits,
        )?);
    }
    Ok(cases)
}

fn confirmation_cases(
    scope: WorkbenchQuery,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let mut cases = Vec::new();
    let metadata = WorkbenchFileMetadata::new(
        Sha256Digest::new([63; 32]),
        20,
        (6, 14),
        Sha256Digest::new([64; 32]),
    )
    .expect("metadata");
    let preview = WorkbenchFilePreview::new(
        WorkbenchFileRequest::new(
            scope,
            9,
            "src/reference.txt".to_owned(),
            WorkbenchFileRange::Lines { first: 2, last: 2 },
            WorkbenchFileMode::RefreshOnRequest,
            id(61, ProviderProfileId::new),
            ProductModelChoice::default(),
        )
        .expect("request"),
        Sha256Digest::new([62; 32]),
        metadata,
        2,
        "configured-model".to_owned(),
    )
    .expect("preview");
    let command = WorkbenchCommand::new(
        id(64, ControlOperationId::new),
        scope,
        9,
        WorkbenchIntent::AttachFile {
            preview: preview.clone(),
            text: WorkbenchInputText::new("Use this exact selection".to_owned()).expect("caption"),
        },
    );
    for (name, payload) in [
        ("realistic-workbench-file-confirm", AppRequestPayload::WorkbenchCommand(command.clone())),
        (
            "realistic-workbench-file-receipt-query",
            AppRequestPayload::QueryWorkbenchReceipt(command),
        ),
        (
            "minimal-workbench-file-page-query",
            AppRequestPayload::QueryWorkbenchFiles(
                WorkbenchFileQuery::new(scope, 10, 0).expect("query"),
            ),
        ),
        (
            "minimal-workbench-file-deselect",
            AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                id(65, ControlOperationId::new),
                scope,
                10,
                WorkbenchIntent::SelectFile {
                    attachment: id(64, ControlOperationId::new),
                    selected: false,
                },
            )),
        ),
    ] {
        cases.push(encoded(name, FixtureClass::Realistic, &request(payload), limits)?);
    }
    let page = WorkbenchFilePage::new(
        WorkbenchFileQuery::new(scope, 11, 0).expect("query"),
        vec![
            WorkbenchFileRow::new(
                id(64, ControlOperationId::new),
                id(66, ControlOperationId::new),
                "src/reference.txt".to_owned(),
                WorkbenchFileMode::RefreshOnRequest,
                metadata,
                false,
                false,
            )
            .expect("row"),
        ],
        1,
    )
    .expect("page");
    for (name, payload) in [
        ("realistic-workbench-file-preview", AppResponsePayload::WorkbenchFilePreview(preview)),
        ("realistic-workbench-file-page", AppResponsePayload::WorkbenchFiles(page)),
    ] {
        cases.push(encoded(
            name,
            FixtureClass::Realistic,
            &AppResponseEnvelope::new(
                context(),
                id(10, RequestId::new),
                id(11, CorrelationId::new),
                payload,
            ),
            limits,
        )?);
    }
    Ok(cases)
}
