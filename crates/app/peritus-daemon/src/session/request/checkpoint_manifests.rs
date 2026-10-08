//! Negotiate complete checkpoint representations before accepting any user filesystem effect.

use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppResponsePayload, WorkbenchCommand, WorkbenchIntent,
};

pub(super) fn command_supported(command: &WorkbenchCommand, supported: bool) -> bool {
    supported
        || match command.intent() {
            WorkbenchIntent::CreateCheckpoint(name) => !name.requires_manifest_feature(),
            WorkbenchIntent::ApplyRewind(preview) => !preview.requires_manifest_feature(),
            _ => true,
        }
}

pub(super) fn response(payload: AppResponsePayload, supported: bool) -> AppResponsePayload {
    let needs_feature = match &payload {
        AppResponsePayload::WorkbenchCheckpoint(value) => value.requires_manifest_feature(),
        AppResponsePayload::WorkbenchRewindPreview(value) => value.requires_manifest_feature(),
        AppResponsePayload::WorkbenchRestore(value) => value.requires_manifest_feature(),
        _ => false,
    };
    if !supported && needs_feature {
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::MissingRequiredFeature, None))
    } else {
        payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::{
        ControlOperationId, ConversationId, WorkbenchCheckpointName, WorkbenchQuery,
    };

    #[test]
    fn wide_mutations_require_negotiation_and_ordinary_mutations_keep_legacy_compatibility() {
        let query = WorkbenchQuery::new(
            ConversationId::new([1; 16]).unwrap(),
            peritus_types::WorkspaceId::new([2; 16]).unwrap(),
        );
        for (name, wide) in [("ordinary".to_owned(), false), ("é".repeat(129), true)] {
            let command = WorkbenchCommand::new(
                ControlOperationId::new([3; 16]).unwrap(),
                query,
                1,
                WorkbenchIntent::CreateCheckpoint(WorkbenchCheckpointName::new(name).unwrap()),
            );
            assert!(command_supported(&command, true));
            assert_eq!(command_supported(&command, false), !wide);
        }
    }
}
