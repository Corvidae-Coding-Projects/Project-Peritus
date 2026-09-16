//! Exact control-field transitions used by reduction and replay.
//!
//! Recovery checks are an explicitly supplied fact. Completing cancellation here changes the
//! aggregate phase only; termination and reconciliation of external effects remain caller duties.

use crate::{ActivePhase, AgentPhase, TerminalKind};
use vstd::prelude::*;

verus! {

/// Rejection classification, translated into the existing reducer error at its ordinary boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRejection {
    IllegalPhase,
    RecoveryUnchecked,
    NoResumablePhase,
}

pub const fn pause(
    phase: &mut AgentPhase,
    paused_from: &mut Option<ActivePhase>,
) -> (result: Result<(), ControlRejection>)
    ensures
        match *old(phase) {
            AgentPhase::Active(active) => result == Ok(())
                && *final(phase) == AgentPhase::Paused
                && *final(paused_from) == Some(active),
            _ => result == Err(ControlRejection::IllegalPhase)
                && *final(phase) == *old(phase)
                && *final(paused_from) == *old(paused_from),
        },
{
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
) -> (result: Result<(), ControlRejection>)
    ensures
        match *old(phase) {
            AgentPhase::Paused => {
                if !recovery_checked {
                    result == Err(ControlRejection::RecoveryUnchecked)
                } else {
                    match *old(paused_from) {
                        Some(active) => result == Ok(())
                            && *final(phase) == AgentPhase::Active(active)
                            && *final(paused_from) == None,
                        None => result == Err(ControlRejection::NoResumablePhase),
                    }
                }
            },
            _ => result == Err(ControlRejection::IllegalPhase),
        },
        result.is_err() ==> *final(phase) == *old(phase)
            && *final(paused_from) == *old(paused_from),
{
    match *phase {
        AgentPhase::Paused => {},
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
) -> (result: Result<(), ControlRejection>)
    ensures
        match *old(phase) {
            AgentPhase::Active(_) | AgentPhase::Paused => result == Ok(())
                && *final(phase) == AgentPhase::Cancelling
                && *final(paused_from) == None,
            _ => result == Err(ControlRejection::IllegalPhase)
                && *final(phase) == *old(phase)
                && *final(paused_from) == *old(paused_from),
        },
{
    match *phase {
        AgentPhase::Active(_) | AgentPhase::Paused => {},
        _ => return Err(ControlRejection::IllegalPhase),
    }
    *paused_from = None;
    *phase = AgentPhase::Cancelling;
    Ok(())
}

pub const fn finish_cancellation(
    phase: &mut AgentPhase,
) -> (result: Result<(), ControlRejection>)
    ensures
        match *old(phase) {
            AgentPhase::Cancelling => result == Ok(())
                && *final(phase) == AgentPhase::Terminal(TerminalKind::Cancelled),
            _ => result == Err(ControlRejection::IllegalPhase)
                && *final(phase) == *old(phase),
        },
{
    match *phase {
        AgentPhase::Cancelling => {},
        _ => return Err(ControlRejection::IllegalPhase),
    }
    *phase = AgentPhase::Terminal(TerminalKind::Cancelled);
    Ok(())
}

} // verus!

#[cfg(test)]
mod tests;
