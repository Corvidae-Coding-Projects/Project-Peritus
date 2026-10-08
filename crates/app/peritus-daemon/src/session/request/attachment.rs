//! Register and attach terminal ownership before recording connection cleanup bindings.

use super::{AppResponsePayload, ProductRunService, TerminalRegistry};
use peritus_app_protocol::TerminalBinding;

pub(super) async fn attach(
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
    let product_runs = product_runs.clone();
    let terminals = terminals.clone();
    let registration = super::blocking::protocol_result(move || {
        product_runs
            .register_preview_terminal(actor, session, binding.process_id(), &terminals)
            .and_then(|()| {
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
            })
    })
    .await;
    match registration {
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
