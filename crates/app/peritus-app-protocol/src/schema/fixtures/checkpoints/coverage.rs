//! New typed coverage fixtures coexist with unchanged legacy file-only frame bytes.

use super::{FixtureClass, GeneratedFixtureCase, response, version};
use crate::{
    AppResponsePayload, ControlOperationId, WorkbenchCheckpointName, WorkbenchCheckpointPath,
    WorkbenchCheckpointRange, WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences,
    WorkbenchCheckpointVersion, WorkbenchFileRange, WorkbenchQuery, WorkbenchRewindDisposition,
    WorkbenchRewindPath, WorkbenchRewindPreview, WorkbenchRewindRequest,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(
    query: WorkbenchQuery,
    checkpoint: ControlOperationId,
    external: &[String],
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let file = version(60, 18);
    let owned = version(61, 22);
    let ranges = vec![
        WorkbenchCheckpointRange::new(WorkbenchFileRange::Bytes { start: 1, end: 5 }, 1, 5)
            .expect("captured selection"),
    ];
    let directory = WorkbenchCheckpointVersion::EmptyDirectory { permissions: 0o750 };
    let receipt = WorkbenchCheckpointReceipt::new(
        checkpoint,
        query,
        8,
        WorkbenchCheckpointName::new("selected content and empty directory".to_owned())
            .expect("name"),
        WorkbenchCheckpointReferences::new(7, 3, 2, Some(4)),
        vec![
            WorkbenchCheckpointPath::new(
                "empty".to_owned(),
                directory,
                Some(WorkbenchCheckpointVersion::Absent),
            )
            .expect("directory"),
            WorkbenchCheckpointPath::new("src/note.txt".to_owned(), file, Some(owned))
                .expect("file")
                .with_ranges(ranges.clone())
                .expect("scope"),
        ],
        Vec::new(),
        external.to_vec(),
    )
    .expect("typed receipt");
    let preview = WorkbenchRewindPreview::new(
        WorkbenchRewindRequest::new(query, 9, checkpoint).expect("request"),
        vec![
            WorkbenchRewindPath::new(
                "empty".to_owned(),
                directory,
                Some(WorkbenchCheckpointVersion::Absent),
                WorkbenchCheckpointVersion::Absent,
                WorkbenchRewindDisposition::Restore,
            )
            .expect("directory restore"),
            WorkbenchRewindPath::new_with_ranges(
                "src/note.txt".to_owned(),
                file,
                Some(owned),
                owned,
                WorkbenchRewindDisposition::Restore,
                ranges,
            )
            .expect("range restore"),
        ],
        Vec::new(),
        external.to_vec(),
    )
    .expect("typed preview");
    Ok(vec![
        super::encoded(
            "realistic-workbench-checkpoint-typed-coverage",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchCheckpoint(receipt)),
            limits,
        )?,
        super::encoded(
            "realistic-workbench-rewind-typed-coverage",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRewindPreview(preview)),
            limits,
        )?,
    ])
}
