//! Exact control-field transitions used by reduction and replay.
//!
//! Recovery checks are an explicitly supplied fact. Completing cancellation here changes the
//! aggregate phase only; termination and reconciliation of external effects remain caller duties.

use crate::{ActivePhase, AgentPhase, TerminalKind};

#[doc = " Rejection classification, translated into the existing reducer error at its ordinary boundary."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRejection {
    IllegalPhase,
    RecoveryUnchecked,
    NoResumablePhase,
}
pub const fn pause(
    phase: &mut AgentPhase,
    paused_from: &mut Option<ActivePhase>,
) -> Result<(), ControlRejection> {
    let AgentPhase::Active(active) = *phase else {
        return Err(ControlRejection::IllegalPhase);
    };
    *paused_from = Some(active);
    *phase = AgentPhase::Paused;
    Ok(())
}
pub const fn resume(
    phase: &mut AgentPhase,
    paused_from: &mut Option<ActivePhase>,
    recovery_checked: bool,
) -> Result<(), ControlRejection> {
    match *phase {
        AgentPhase::Paused => {}
        _ => return Err(ControlRejection::IllegalPhase),
    }
    if !recovery_checked {
        return Err(ControlRejection::RecoveryUnchecked);
    }
    let Some(active) = *paused_from else {
        return Err(ControlRejection::NoResumablePhase);
    };
    *paused_from = None;
    *phase = AgentPhase::Active(active);
    Ok(())
}
pub const fn request_cancellation(
    phase: &mut AgentPhase,
    paused_from: &mut Option<ActivePhase>,
) -> Result<(), ControlRejection> {
    match *phase {
        AgentPhase::Active(_) | AgentPhase::Paused => {}
        _ => return Err(ControlRejection::IllegalPhase),
    }
    *paused_from = None;
    *phase = AgentPhase::Cancelling;
    Ok(())
}
pub const fn finish_cancellation(phase: &mut AgentPhase) -> Result<(), ControlRejection> {
    match *phase {
        AgentPhase::Cancelling => {}
        _ => return Err(ControlRejection::IllegalPhase),
    }
    *phase = AgentPhase::Terminal(TerminalKind::Cancelled);
    Ok(())
}
