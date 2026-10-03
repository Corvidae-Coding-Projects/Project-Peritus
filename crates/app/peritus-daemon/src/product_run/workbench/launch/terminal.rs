//! Authenticated terminal access to the exact conversation-owned preview process.

use super::{
    ActorId, AppProtocolError, Code, ControlOperationId, ProductRunService, SessionId,
    WorkbenchQuery, app_error, error_value,
};
use peritus_product_runner::control::PermissionCapability;
use peritus_types::ProcessId;

impl ProductRunService {
    fn preview_terminal_scope(
        &self,
        process: ProcessId,
    ) -> Result<Option<(WorkbenchQuery, ControlOperationId)>, AppProtocolError> {
        let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
        Ok(records.values().filter_map(|record| record.preview.page.as_ref()).find_map(|page| {
            page.launches()
                .iter()
                .find(|launch| launch.process() == Some(process))
                .map(|launch| (page.query().query(), launch.launch()))
        }))
    }

    pub(crate) fn authorize_preview_terminal_input(
        &self,
        actor: ActorId,
        process: ProcessId,
    ) -> Result<(), AppProtocolError> {
        if let Some((query, _)) = self.preview_terminal_scope(process)? {
            use PermissionCapability::{Network, Process, Read, Write};
            self.require_workspace_permissions(actor, query, &[Read, Write, Process, Network])
                .map_err(error_value)?;
        }
        Ok(())
    }

    pub(crate) fn register_preview_terminal(
        &self,
        actor: ActorId,
        session: SessionId,
        process: ProcessId,
        terminals: &crate::terminal::TerminalRegistry,
    ) -> Result<(), AppProtocolError> {
        let Some((query, launch)) = self.preview_terminal_scope(process)? else { return Ok(()) };
        self.require_workspace_permissions(actor, query, &[PermissionCapability::Read])
            .map_err(error_value)?;
        let active = self.preview_process(launch)?;
        let lease = active
            .runtime
            .preview_terminal(&active.launch)
            .map_err(|_| app_error(Code::InvalidIdentifier))?;
        terminals.register_preview(actor, session, lease).map_err(|error| error.protocol_error())
    }
}
