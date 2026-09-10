//! Image-page fixtures include an eligible reference and a deselected retained reference.

use super::{FixtureClass, GeneratedFixtureCase, context, encoded, id, request};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, CorrelationId,
    RequestId, WorkbenchImageFormat, WorkbenchImageLabel, WorkbenchImageMetadata,
    WorkbenchImagePage, WorkbenchImageQuery, WorkbenchImageRow, WorkbenchInputId,
    WorkbenchInputOrder, WorkbenchInputRow, WorkbenchInputSelection, WorkbenchInputState,
    WorkbenchInputText, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{ArtifactId, Sha256Digest};

pub(super) fn cases(
    scope: WorkbenchQuery,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let mut rows = Vec::new();
    for (seed, selected) in [(70, true), (71, false)] {
        rows.push(
            WorkbenchImageRow::new(
                id(seed, ControlOperationId::new),
                id(seed, ArtifactId::new),
                WorkbenchImageLabel::new(format!("reference-{seed}.gif")).expect("label"),
                WorkbenchImageMetadata::new(
                    Sha256Digest::new([seed; 32]),
                    35,
                    WorkbenchImageFormat::Gif,
                    (1, 1),
                    1,
                )
                .expect("metadata"),
                WorkbenchInputRow::new(
                    WorkbenchInputSelection::new(id(seed, WorkbenchInputId::new), 1)
                        .expect("source"),
                    WorkbenchInputText::new("Use this exact reference".to_owned())
                        .expect("caption"),
                    WorkbenchInputState::Queued,
                    WorkbenchInputOrder::new(vec![]).expect("dependencies"),
                )
                .expect("row"),
                (selected, selected),
            )
            .expect("image"),
        );
    }
    let page = WorkbenchImagePage::new(
        WorkbenchImageQuery::new(scope, 12, 0).expect("page query"),
        2,
        rows,
    )
    .expect("page");
    Ok(vec![
        encoded(
            "minimal-workbench-image-page-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryWorkbenchImages(
                WorkbenchImageQuery::new(scope, 0, 0).expect("query"),
            )),
            limits,
        )?,
        encoded(
            "realistic-workbench-image-page",
            FixtureClass::Realistic,
            &AppResponseEnvelope::new(
                context(),
                id(10, RequestId::new),
                id(11, CorrelationId::new),
                AppResponsePayload::WorkbenchImages(page),
            ),
            limits,
        )?,
    ])
}
