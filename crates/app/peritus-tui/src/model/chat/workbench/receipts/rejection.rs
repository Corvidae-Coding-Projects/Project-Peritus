//! Rejected admission must not lock the user out of the controls needed for recovery.

use super::{AppModel, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::AppErrorCode;

impl AppModel {
    pub(in crate::model) fn resolve_rejected_control(
        &mut self,
        pending: Option<&PendingRequest>,
        code: AppErrorCode,
    ) -> Option<Vec<Effect>> {
        let pending = pending?;
        let (PendingRequest::WorkbenchControl(command) | PendingRequest::WorkbenchReceipt(command)) =
            pending
        else {
            return None;
        };
        if !self.chat.workbench.unresolved.as_ref().is_some_and(|(expected, _)| expected == command)
        {
            return None;
        }
        if matches!(pending, PendingRequest::WorkbenchControl(_))
            && matches!(
                code,
                AppErrorCode::ReadOnly
                    | AppErrorCode::InvalidIdentifier
                    | AppErrorCode::LimitExceeded
            )
        {
            // An earlier transport failure might have hidden acceptance. Resolve the exact
            // operation before releasing its fence; never replay the effect to find out.
            self.chat.workbench.rejected_control = Some(code);
            return Some(self.recover_workbench_receipt());
        }
        if matches!(pending, PendingRequest::WorkbenchReceipt(_))
            && code == AppErrorCode::InvalidIdentifier
            && let Some(rejected) = self.chat.workbench.rejected_control.take()
        {
            self.workbench_error(
                Some(&PendingRequest::WorkbenchControl(command.clone())),
                rejected,
            );
            self.notice(
                NoticeLevel::Warning,
                format!(
                    "Command not accepted: {}. Draft retained; recovery controls are available.",
                    rejected.as_str()
                ),
            );
            return Some(Vec::new());
        }
        None
    }
}
