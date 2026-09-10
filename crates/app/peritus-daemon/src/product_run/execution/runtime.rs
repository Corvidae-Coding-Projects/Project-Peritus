//! Command runtime construction for exact managed and direct-folder capabilities.
use super::{ProductRunService, launch::run_hex};
use peritus_app_protocol::ProductRunRequest;
use peritus_product_runner::{CommandRuntime, ProductRunnerError};
use std::path::Path;
pub(super) fn open(
    service: &ProductRunService,
    request: &ProductRunRequest,
    root: &Path,
) -> Result<CommandRuntime, ProductRunnerError> {
    let run_id = request.run_id();
    let state = service.inner.directory.join("commands").join(run_hex(run_id));
    let runtime = if service.inner.folders.contains_key(&request.workspace_id()) {
        CommandRuntime::open_direct(state, root.to_owned(), run_id, service.inner.processes.clone())
    } else {
        CommandRuntime::open(state, root, run_id, service.inner.processes.clone())
    }?;
    runtime.with_local_context(service.inner.local_context.clone())
}
