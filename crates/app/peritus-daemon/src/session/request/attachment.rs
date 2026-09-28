//! Register and attach terminal ownership before recording connection cleanup bindings.

use super::{AppProtocolLimits, AppResponsePayload, ProductRunService, TerminalRegistry};
use peritus_app_protocol::TerminalBinding;
use peritus_types::{ActorId, SessionId};

pub(super) fn attach(
    product_runs: &ProductRunService,
    terminals: &TerminalRegistry,
    actor: ActorId,
    session: SessionId,
    binding: TerminalBinding,
    limits: AppProtocolLimits,
    bindings: &mut Vec<TerminalBinding>,
) -> AppResponsePayload {
    let registration =
        product_runs.register_preview_terminal(actor, session, binding.process_id(), terminals);
    match registration.and_then(|()| {
        terminals
            .attach(actor, session, binding, limits.max_terminal_chunk_bytes())
            .map_err(|error| error.protocol_error())
    }) {
        Ok(_) => {
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
            AppResponsePayload::TerminalAttached(binding)
        }
        Err(error) => AppResponsePayload::Error(error),
    }
}
