//! Keep legacy peers on their exact checkpoint schema before any filesystem effect.

use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppResponsePayload, WorkbenchCheckpointVersion,
    WorkbenchCommand, WorkbenchIntent, WorkbenchRewindPreview,
};

const fn enhanced(version: WorkbenchCheckpointVersion) -> bool {
    matches!(version, WorkbenchCheckpointVersion::EmptyDirectory { .. })
}

fn enhanced_preview(preview: &WorkbenchRewindPreview) -> bool {
    preview.paths().iter().any(|path| {
        !path.ranges().is_empty()
            || enhanced(path.checkpoint())
            || enhanced(path.observed_current())
            || path.expected_current().is_some_and(enhanced)
    })
}

pub(super) fn command_supported(command: &WorkbenchCommand, supported: bool) -> bool {
    supported
        || !matches!(command.intent(), WorkbenchIntent::ApplyRewind(preview)
        if enhanced_preview(preview))
}

pub(super) fn response(payload: AppResponsePayload, supported: bool) -> AppResponsePayload {
    if supported {
        return payload;
    }
    let needs_feature = match &payload {
        AppResponsePayload::WorkbenchCheckpoint(checkpoint) => {
            checkpoint.paths().iter().any(|path| {
                !path.ranges().is_empty()
                    || enhanced(path.checkpoint())
                    || path.expected_current().is_some_and(enhanced)
            })
        }
        AppResponsePayload::WorkbenchRewindPreview(preview) => enhanced_preview(preview),
        _ => false,
    };
    if needs_feature {
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::MissingRequiredFeature, None))
    } else {
        payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::{
        ControlOperationId, WorkbenchCheckpointFileMode, WorkbenchCheckpointRange,
        WorkbenchFileRange, WorkbenchQuery, WorkbenchRewindDisposition, WorkbenchRewindPath,
        WorkbenchRewindRequest,
    };

    #[test]
    fn legacy_peers_keep_file_only_previews_and_typed_mutations_require_the_negotiated_schema() {
        let query = WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([1; 16]).unwrap(),
            peritus_types::WorkspaceId::new([2; 16]).unwrap(),
        );
        let request =
            WorkbenchRewindRequest::new(query, 1, ControlOperationId::new([3; 16]).unwrap())
                .unwrap();
        let saved = WorkbenchCheckpointVersion::Present {
            digest: peritus_types::Sha256Digest::new([4; 32]),
            bytes: 10,
            mode: WorkbenchCheckpointFileMode::Regular,
        };
        let current = WorkbenchCheckpointVersion::Present {
            digest: peritus_types::Sha256Digest::new([5; 32]),
            bytes: 10,
            mode: WorkbenchCheckpointFileMode::Regular,
        };
        for ranges in [
            Vec::new(),
            vec![
                WorkbenchCheckpointRange::new(WorkbenchFileRange::Bytes { start: 1, end: 3 }, 1, 3)
                    .unwrap(),
            ],
        ] {
            let enhanced = !ranges.is_empty();
            let preview = WorkbenchRewindPreview::new(
                request,
                vec![
                    WorkbenchRewindPath::new_with_ranges(
                        "node".to_owned(),
                        saved,
                        Some(current),
                        current,
                        WorkbenchRewindDisposition::Restore,
                        ranges,
                    )
                    .unwrap(),
                ],
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
            let payload = AppResponsePayload::WorkbenchRewindPreview(preview.clone());
            assert_eq!(response(payload.clone(), true), payload);
            assert_eq!(response(payload.clone(), false) == payload, !enhanced);
            let command = WorkbenchCommand::new(
                ControlOperationId::new([6; 16]).unwrap(),
                query,
                1,
                WorkbenchIntent::ApplyRewind(preview),
            );
            assert!(command_supported(&command, true));
            assert_eq!(command_supported(&command, false), !enhanced);
        }
    }
}
