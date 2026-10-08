//! Owned adapters for synchronous request work that may wait on storage or host I/O.

use super::{
    AppErrorCode, AppProtocolError, AppRequestPayload, AppResponsePayload, ProductRunService,
};
use peritus_app_protocol::WorkbenchIntent;

pub(super) async fn authorize(
    service: &ProductRunService,
    actor: peritus_types::ActorId,
    request: &AppRequestPayload,
) -> Result<(), AppProtocolError> {
    if !authorization_may_block(request) {
        return service.authorize_workbench_request(actor, request);
    }
    let service = service.clone();
    let request = request.clone();
    protocol_result(move || service.authorize_workbench_request(actor, &request)).await
}

pub(super) async fn response(
    operation: impl FnOnce() -> AppResponsePayload + Send + 'static,
) -> AppResponsePayload {
    match protocol(operation).await {
        Ok(payload) => payload,
        Err(error) => AppResponsePayload::Error(error),
    }
}

pub(super) async fn protocol<T>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, AppProtocolError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|_| AppProtocolError::new(AppErrorCode::Internal, None))
}

pub(super) async fn protocol_result<T>(
    operation: impl FnOnce() -> Result<T, AppProtocolError> + Send + 'static,
) -> Result<T, AppProtocolError>
where
    T: Send + 'static,
{
    protocol(operation).await?
}

fn authorization_may_block(request: &AppRequestPayload) -> bool {
    match request {
        AppRequestPayload::TerminalInput(_)
        | AppRequestPayload::PreviewWorkbenchFileImport(_)
        | AppRequestPayload::PreviewWorkbenchFile(_)
        | AppRequestPayload::QueryWorkbenchFiles(_)
        | AppRequestPayload::QueryWorkbenchImages(_)
        | AppRequestPayload::PreviewWorkbenchImage(_)
        | AppRequestPayload::InspectWorkbenchCheckpoint(_)
        | AppRequestPayload::PreviewWorkbenchRewind(_)
        | AppRequestPayload::DiscoverInit(_) => true,
        AppRequestPayload::WorkbenchCommand(command) => {
            command_authorization_may_block(command.intent())
        }
        _ => false,
    }
}

fn command_authorization_may_block(intent: &WorkbenchIntent) -> bool {
    matches!(
        intent,
        WorkbenchIntent::CreateCheckpoint(_)
            | WorkbenchIntent::ForkConversation(_)
            | WorkbenchIntent::ApplyRewind(_)
            | WorkbenchIntent::ApplyInitDiff(_)
            | WorkbenchIntent::AttachFile { .. }
            | WorkbenchIntent::AttachFileImport { .. }
            | WorkbenchIntent::AttachFileSource { .. }
            | WorkbenchIntent::AttachImage { .. }
            | WorkbenchIntent::StartPreview(_)
            | WorkbenchIntent::InteractPreview { .. }
            | WorkbenchIntent::CapturePreview(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::{
        ControlOperationId, ProductRunControl, ProductRunControlAction, WorkbenchCheckpointName,
    };
    use peritus_types::RunId;

    #[test]
    fn cancellation_commands_do_not_queue_behind_blocking_authorization() {
        let operation = ControlOperationId::new([1; 16]).unwrap();
        let control = AppRequestPayload::ControlProductRun(ProductRunControl::new(
            RunId::new([2; 16]).unwrap(),
            ProductRunControlAction::Cancel,
        ));
        assert!(!authorization_may_block(&control));
        assert!(!authorization_may_block(&AppRequestPayload::DaemonStatus));
        assert!(!command_authorization_may_block(&WorkbenchIntent::StopPreview {
            launch: operation,
        }));
        assert!(command_authorization_may_block(&WorkbenchIntent::CreateCheckpoint(
            WorkbenchCheckpointName::new("checkpoint".to_owned()).unwrap(),
        )));
    }
}
