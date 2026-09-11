//! Scoped image transfer and provider-bound confirmation fixture coverage.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ArtifactMetadata,
    CanonicalMediaType, ControlOperationId, ConversationId, CorrelationId, ProductModelChoice,
    RequestId, TransferId, WorkbenchCommand, WorkbenchImageFormat, WorkbenchImageLabel,
    WorkbenchImageMetadata, WorkbenchImagePreview, WorkbenchImageRequest, WorkbenchImageUpload,
    WorkbenchInputText, WorkbenchIntent, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{ArtifactId, ProviderProfileId, Sha256Digest, WorkspaceId};

mod page;

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let scope = WorkbenchQuery::new(id(41, ConversationId::new), id(32, WorkspaceId::new));
    let artifact = id(60, ArtifactId::new);
    let selection = WorkbenchImageRequest::new(
        scope,
        9,
        artifact,
        id(61, ProviderProfileId::new),
        ProductModelChoice::default(),
        WorkbenchImageLabel::new("explicit reference.gif".to_owned()).expect("label"),
    )
    .expect("selection");
    let metadata = ArtifactMetadata::new(
        id(62, TransferId::new),
        artifact,
        35,
        CanonicalMediaType::new("image/gif".to_owned(), 128).expect("MIME"),
        Sha256Digest::new([63; 32]),
        1024,
        1024,
    )
    .expect("transfer");
    let upload = WorkbenchImageUpload::new(scope, 9, metadata).expect("upload");
    let mut cases = vec![
        encoded(
            "minimal-workbench-image-upload",
            FixtureClass::Minimal,
            &request(AppRequestPayload::BeginWorkbenchImageUpload(upload)),
            limits,
        )?,
        encoded(
            "minimal-workbench-image-preview-request",
            FixtureClass::Minimal,
            &request(AppRequestPayload::PreviewWorkbenchImage(selection.clone())),
            limits,
        )?,
    ];
    cases.extend(preview_cases(&selection, limits)?);
    cases.extend(confirmation_cases(scope, selection, limits)?);
    cases.extend(page::cases(scope, limits)?);
    Ok(cases)
}

fn preview_cases(
    selection: &WorkbenchImageRequest,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let mut cases = Vec::new();
    for (name, format) in [
        ("realistic-workbench-image-png-preview", WorkbenchImageFormat::Png),
        ("realistic-workbench-image-jpeg-preview", WorkbenchImageFormat::Jpeg),
        ("realistic-workbench-image-gif-preview", WorkbenchImageFormat::Gif),
        ("realistic-workbench-image-webp-preview", WorkbenchImageFormat::Webp),
    ] {
        let preview = WorkbenchImagePreview::new(
            selection.clone(),
            WorkbenchImageMetadata::new(Sha256Digest::new([63; 32]), 35, format, (1, 1), 1)
                .expect("metadata"),
            2,
            "configured-vision-model".to_owned(),
        )
        .expect("preview");
        cases.push(encoded(
            name,
            FixtureClass::Realistic,
            &AppResponseEnvelope::new(
                context(),
                id(10, RequestId::new),
                id(11, CorrelationId::new),
                AppResponsePayload::WorkbenchImagePreview(preview),
            ),
            limits,
        )?);
    }
    Ok(cases)
}

fn confirmation_cases(
    scope: WorkbenchQuery,
    selection: WorkbenchImageRequest,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let mut cases = Vec::new();
    let preview = WorkbenchImagePreview::new(
        selection,
        WorkbenchImageMetadata::new(
            Sha256Digest::new([63; 32]),
            35,
            WorkbenchImageFormat::Gif,
            (1, 1),
            1,
        )
        .expect("metadata"),
        2,
        "configured-vision-model".to_owned(),
    )
    .expect("preview");
    let command = WorkbenchCommand::new(
        id(64, ControlOperationId::new),
        scope,
        9,
        WorkbenchIntent::AttachImage {
            preview,
            text: WorkbenchInputText::new("Use this exact image".to_owned()).expect("caption"),
        },
    );
    for (name, payload) in [
        ("realistic-workbench-image-confirm", AppRequestPayload::WorkbenchCommand(command.clone())),
        (
            "realistic-workbench-image-receipt-query",
            AppRequestPayload::QueryWorkbenchReceipt(command),
        ),
        (
            "minimal-workbench-image-deselect",
            AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                id(65, ControlOperationId::new),
                scope,
                10,
                WorkbenchIntent::SelectImage {
                    attachment: id(64, ControlOperationId::new),
                    selected: false,
                },
            )),
        ),
    ] {
        cases.push(encoded(name, FixtureClass::Realistic, &request(payload), limits)?);
    }
    Ok(cases)
}
