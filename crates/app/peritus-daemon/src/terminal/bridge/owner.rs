//! Distinct direct and C4 lifecycle owners; a preview lease never takes the process from C4.

use super::{LiveTerminalRegistration, TerminalBridge, rejected};
use crate::terminal::{TerminalBridgeError, TerminalBridgeErrorKind};
use peritus_process::{OwnedProcess, TerminalResult};
use peritus_product_runner::PreviewTerminal;
use peritus_types::{ActorId, SessionId};

pub(super) enum TerminalOwner {
    Direct(OwnedProcess),
    Preview(PreviewTerminal),
}

impl TerminalOwner {
    pub(super) fn wait(self) -> Result<TerminalResult, TerminalBridgeError> {
        match self {
            Self::Direct(owner) => owner.wait().map_err(Into::into),
            Self::Preview(owner) => owner.wait().map_err(TerminalBridgeError::Preview),
        }
    }
}

impl LiveTerminalRegistration {
    pub(in crate::terminal) fn preview(
        actor: ActorId,
        session: SessionId,
        lease: PreviewTerminal,
    ) -> Result<Self, TerminalBridgeError> {
        let plan = lease.plan();
        Self::validate_observation_bounds(plan)?;
        let control = lease.control();
        let birth_identity = control
            .tree_identity()
            .filter(|birth| {
                birth.root_pid() != 0
                    && birth.start_token().is_some()
                    && birth.complete_containment()
            })
            .ok_or_else(|| {
                rejected(
                    TerminalBridgeErrorKind::BirthIdentityUnavailable,
                    "preview has no exact contained birth identity",
                )
            })?;
        Ok(Self {
            actor_id: actor,
            session_id: session,
            process_id: plan.identity().process_id(),
            plan_digest: plan.digest(),
            io_mode: plan.io_mode(),
            birth_identity,
            control,
            owner: TerminalOwner::Preview(lease),
        })
    }
}

impl TerminalBridge {
    pub(in crate::terminal) fn reconnect_preview(
        &mut self,
        actor: ActorId,
        session: SessionId,
    ) -> Result<(), TerminalBridgeError> {
        if self.actor_id != actor || !matches!(self.owner, TerminalOwner::Preview(_)) {
            return Err(rejected(
                TerminalBridgeErrorKind::OwnershipMismatch,
                "preview owner differs",
            ));
        }
        if self.session_id != session && !self.attachments.is_empty() {
            return Err(rejected(
                TerminalBridgeErrorKind::RegistrationConflict,
                "detach the preview terminal from its other connection before reattaching",
            ));
        }
        self.session_id = session;
        Ok(())
    }
}
