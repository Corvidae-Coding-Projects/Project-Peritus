//! Exact E0 publish-command claim binding for all seven directive destinations.

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::OutboxMessage;
use peritus_orchestrator::{
    DirectiveDestination, OrchestratorCommand, OrchestratorCommandFrame, OrchestratorCommandKind,
    PendingDirective,
};

use crate::DaemonError;

use super::{domain_claim_error, invalid_claim, require_claimed};

/// Syntax-checked E0 publish command, exact directive, and positive C0 claim fence.
///
/// This value is inert: decoding it neither publishes the directive nor creates acknowledgement
/// or append authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorDirectiveClaim {
    command: OrchestratorCommand,
    directive: PendingDirective,
    fence: u64,
}

impl OrchestratorDirectiveClaim {
    /// Borrows the exact canonical publish command.
    pub(crate) const fn command(&self) -> &OrchestratorCommand {
        &self.command
    }

    /// Borrows the exact inert directive selected by the outbox destination.
    pub(crate) const fn directive(&self) -> &PendingDirective {
        &self.directive
    }

    /// Returns the positive C0 claim fence.
    pub(crate) const fn fence(&self) -> u64 {
        self.fence
    }

    /// Consumes the claim into the command, directive, and fence needed by a typed handler.
    pub(crate) fn into_parts(self) -> (OrchestratorCommand, PendingDirective, u64) {
        (self.command, self.directive, self.fence)
    }
}

/// Decodes and binds one E0 outbox claim to an exact expected directive destination.
///
/// # Errors
/// Rejects any non-claimed row, destination or fence mismatch, noncanonical command frame,
/// non-publish command, directive identity mismatch, or delivery-bound mismatch.
pub fn decode_orchestrator_claim(
    message: &OutboxMessage,
    expected_destination: DirectiveDestination,
) -> Result<OrchestratorDirectiveClaim, DaemonError> {
    let destination = expected_destination.outbox_destination();
    let fence = require_claimed(message, destination)?;
    let frame =
        decode_message::<OrchestratorCommandFrame>(message.payload(), CodecLimits::PRODUCTION)
            .map_err(|error| {
                domain_claim_error(
                    "orchestrator outbox payload is not a canonical command frame",
                    error,
                )
            })?;
    let command = frame.into_command();
    let canonical =
        encode_message(&OrchestratorCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
            .map_err(|error| {
                domain_claim_error("orchestrator command cannot be canonically re-encoded", error)
            })?;
    if canonical != message.payload() {
        return Err(invalid_claim(
            "orchestrator outbox payload differs from exact canonical command bytes",
        ));
    }
    let OrchestratorCommandKind::PublishDirective { directive } = command.kind() else {
        return Err(invalid_claim("orchestrator outbox command is not a publish directive"));
    };
    let directive = directive.clone();
    if directive.destination() != expected_destination {
        return Err(invalid_claim(
            "orchestrator directive destination differs from the outbox destination",
        ));
    }
    if directive.id().as_bytes() != message.id().as_bytes() {
        return Err(invalid_claim(
            "orchestrator directive identity differs from the outbox identity",
        ));
    }
    if directive.maximum_deliveries() != message.max_attempts() {
        return Err(invalid_claim(
            "orchestrator directive maximum deliveries differ from outbox maximum attempts",
        ));
    }
    Ok(OrchestratorDirectiveClaim { command, directive, fence })
}
