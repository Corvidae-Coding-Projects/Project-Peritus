//! Reference capability evaluation.

use super::{ProbeDecision, ReferenceProbe, RequestedProcessSignal};
use crate::{
    DescendantPolicy, FileDecision, InputPermission, NetworkDecision, ResizePermission,
    SandboxContract, SignalPolicy, TerminalSignalPermission,
};

pub(super) fn evaluate(contract: &SandboxContract, probe: &ReferenceProbe) -> ProbeDecision {
    let allowed = match probe {
        ReferenceProbe::Filesystem(requirement) => {
            contract.filesystem().decide(requirement.path(), requirement.operation())
                == FileDecision::Allowed
        }
        ReferenceProbe::RootProgram(program) => {
            contract.process().root_programs().contains(program)
        }
        ReferenceProbe::DescendantCount(count) => match contract.process().descendants() {
            DescendantPolicy::Denied => *count == 0,
            DescendantPolicy::Bounded(limit) => *count <= limit,
        },
        ReferenceProbe::ProcessSignal(signal) => matches!(
            (contract.process().signals(), signal),
            (
                SignalPolicy::GracefulOnly | SignalPolicy::GracefulAndForced,
                RequestedProcessSignal::Graceful,
            ) | (SignalPolicy::GracefulAndForced, RequestedProcessSignal::Forced)
        ),
        ReferenceProbe::InheritedEnvironment(name) => {
            contract.environment().permits_inherited(name)
        }
        ReferenceProbe::LiteralEnvironment(name) => contract.environment().permits_literal(name),
        ReferenceProbe::Network(target) => {
            contract.network().decide(target) == NetworkDecision::Allowed
        }
        ReferenceProbe::Secret(requirement) => contract.secrets().permits(requirement),
        ReferenceProbe::Terminal(operation) => match operation {
            crate::RequestedTerminalOperation::Input => {
                contract.terminal().input() == InputPermission::Allowed
            }
            crate::RequestedTerminalOperation::Resize(_) => {
                contract.terminal().resize() == ResizePermission::Allowed
            }
            crate::RequestedTerminalOperation::Signal => {
                contract.terminal().signals() == TerminalSignalPermission::Allowed
            }
        },
    };
    if allowed { ProbeDecision::Allowed } else { ProbeDecision::Denied }
}

pub(super) const fn domain(probe: &ReferenceProbe) -> crate::CapabilityDomain {
    match probe {
        ReferenceProbe::Filesystem(_) => crate::CapabilityDomain::Filesystem,
        ReferenceProbe::RootProgram(_)
        | ReferenceProbe::DescendantCount(_)
        | ReferenceProbe::ProcessSignal(_) => crate::CapabilityDomain::Process,
        ReferenceProbe::InheritedEnvironment(_) | ReferenceProbe::LiteralEnvironment(_) => {
            crate::CapabilityDomain::Environment
        }
        ReferenceProbe::Network(_) => crate::CapabilityDomain::Network,
        ReferenceProbe::Secret(_) => crate::CapabilityDomain::Secret,
        ReferenceProbe::Terminal(_) => crate::CapabilityDomain::Terminal,
    }
}
