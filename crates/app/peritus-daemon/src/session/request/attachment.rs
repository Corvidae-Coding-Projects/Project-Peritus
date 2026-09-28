//! Register and attach terminal ownership before recording connection cleanup bindings.

use super::{AppResponsePayload, ProductRunService, TerminalRegistry};
use peritus_app_protocol::TerminalBinding;

pub(super) fn attach(
    product_runs: &ProductRunService,
    terminals: &TerminalRegistry,
    context: &super::super::negotiation::ConnectionContext,
    binding: TerminalBinding,
    bindings: &mut Vec<TerminalBinding>,
) -> AppResponsePayload {
    let actor = context.actor_id();
    let session = context.protocol().session_id();
    let limits = context.limits();
    let terminal_pipes =
        context.supports(peritus_app_protocol::WellKnownProtocolFeature::TerminalPipes);
    let registration =
        product_runs.register_preview_terminal(actor, session, binding.process_id(), terminals);
    match registration.and_then(|()| {
        let pipes = terminals
            .uses_pipes(actor, session, binding.process_id())
            .map_err(|error| error.protocol_error())?;
        if pipes && !terminal_pipes {
            return Err(peritus_app_protocol::AppProtocolError::new(
                peritus_app_protocol::AppErrorCode::MissingRequiredFeature,
                None,
            ));
        }
        terminals
            .attach(actor, session, binding, limits.max_terminal_chunk_bytes())
            .map(|_| pipes)
            .map_err(|error| error.protocol_error())
    }) {
        Ok(pipes) => {
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
            if pipes {
                AppResponsePayload::TerminalPipeAttached(binding)
            } else {
                AppResponsePayload::TerminalAttached(binding)
            }
        }
        Err(error) => AppResponsePayload::Error(error),
    }
}
