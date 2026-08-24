//! Exhaustive stable tags for terminal canonical values.

use crate::{
    CancellationReason, OutputCompleteness, OutputStream, ProcessError, ProcessResourceDimension,
    ResourceFidelity, TerminalDisposition, TerminalRecovery,
};

use super::corrupt;

pub(super) const fn disposition_tag(value: TerminalDisposition) -> u8 {
    match value {
        TerminalDisposition::Exited => 1,
        TerminalDisposition::Signalled => 2,
        TerminalDisposition::SpawnFailed => 3,
        TerminalDisposition::Cancelled => 4,
        TerminalDisposition::TimedOut => 5,
        TerminalDisposition::OutputLimit => 6,
        TerminalDisposition::ResourceLimit => 7,
        TerminalDisposition::SandboxDenied => 8,
        TerminalDisposition::SupervisorFailed => 9,
        TerminalDisposition::RecoveryIndeterminate => 10,
    }
}

pub(super) const fn decode_disposition(tag: u8) -> Result<TerminalDisposition, ProcessError> {
    match tag {
        1 => Ok(TerminalDisposition::Exited),
        2 => Ok(TerminalDisposition::Signalled),
        3 => Ok(TerminalDisposition::SpawnFailed),
        4 => Ok(TerminalDisposition::Cancelled),
        5 => Ok(TerminalDisposition::TimedOut),
        6 => Ok(TerminalDisposition::OutputLimit),
        7 => Ok(TerminalDisposition::ResourceLimit),
        8 => Ok(TerminalDisposition::SandboxDenied),
        9 => Ok(TerminalDisposition::SupervisorFailed),
        10 => Ok(TerminalDisposition::RecoveryIndeterminate),
        _ => Err(corrupt("terminal result has an unknown disposition")),
    }
}

pub(super) const fn stream_tag(value: OutputStream) -> u8 {
    match value {
        OutputStream::Stdout => 1,
        OutputStream::Stderr => 2,
        OutputStream::Terminal => 3,
    }
}

pub(super) const fn decode_stream(tag: u8) -> Result<OutputStream, ProcessError> {
    match tag {
        1 => Ok(OutputStream::Stdout),
        2 => Ok(OutputStream::Stderr),
        3 => Ok(OutputStream::Terminal),
        _ => Err(corrupt("terminal result has an unknown output stream")),
    }
}

pub(super) const fn completeness_tag(value: OutputCompleteness) -> u8 {
    match value {
        OutputCompleteness::Complete => 1,
        OutputCompleteness::Truncated => 2,
        OutputCompleteness::Incomplete => 3,
    }
}

pub(super) const fn decode_completeness(tag: u8) -> Result<OutputCompleteness, ProcessError> {
    match tag {
        1 => Ok(OutputCompleteness::Complete),
        2 => Ok(OutputCompleteness::Truncated),
        3 => Ok(OutputCompleteness::Incomplete),
        _ => Err(corrupt("terminal result has an unknown output completeness")),
    }
}

pub(super) const fn fidelity_tag(value: ResourceFidelity) -> u8 {
    match value {
        ResourceFidelity::Enforced => 1,
        ResourceFidelity::Sampled => 2,
        ResourceFidelity::Unsupported => 3,
        ResourceFidelity::Incomplete => 4,
    }
}

pub(super) const fn decode_fidelity(tag: u8) -> Result<ResourceFidelity, ProcessError> {
    match tag {
        1 => Ok(ResourceFidelity::Enforced),
        2 => Ok(ResourceFidelity::Sampled),
        3 => Ok(ResourceFidelity::Unsupported),
        4 => Ok(ResourceFidelity::Incomplete),
        _ => Err(corrupt("terminal result has an unknown resource fidelity")),
    }
}

pub(super) const fn resource_tag(value: ProcessResourceDimension) -> u8 {
    match value {
        ProcessResourceDimension::WallTimeMilliseconds => 1,
        ProcessResourceDimension::CpuTimeMilliseconds => 2,
        ProcessResourceDimension::MemoryBytes => 3,
        ProcessResourceDimension::DiskBytes => 4,
        ProcessResourceDimension::OutputBytes => 5,
        ProcessResourceDimension::ProcessCount => 6,
        ProcessResourceDimension::OpenHandles => 7,
        ProcessResourceDimension::ConcurrencySlots => 8,
    }
}

pub(super) const fn decode_resource(tag: u8) -> Result<ProcessResourceDimension, ProcessError> {
    match tag {
        1 => Ok(ProcessResourceDimension::WallTimeMilliseconds),
        2 => Ok(ProcessResourceDimension::CpuTimeMilliseconds),
        3 => Ok(ProcessResourceDimension::MemoryBytes),
        4 => Ok(ProcessResourceDimension::DiskBytes),
        5 => Ok(ProcessResourceDimension::OutputBytes),
        6 => Ok(ProcessResourceDimension::ProcessCount),
        7 => Ok(ProcessResourceDimension::OpenHandles),
        8 => Ok(ProcessResourceDimension::ConcurrencySlots),
        _ => Err(corrupt("terminal result has an unknown resource dimension")),
    }
}

pub(super) const fn reason_tag(value: CancellationReason) -> u8 {
    match value {
        CancellationReason::User => 1,
        CancellationReason::Deadline => 2,
        CancellationReason::OutputLimit => 3,
        CancellationReason::ResourceLimit => 4,
        CancellationReason::LeaseFence => 5,
        CancellationReason::SupervisorShutdown => 6,
        CancellationReason::BackendFailure => 7,
    }
}

pub(super) const fn decode_reason(tag: u8) -> Result<CancellationReason, ProcessError> {
    match tag {
        1 => Ok(CancellationReason::User),
        2 => Ok(CancellationReason::Deadline),
        3 => Ok(CancellationReason::OutputLimit),
        4 => Ok(CancellationReason::ResourceLimit),
        5 => Ok(CancellationReason::LeaseFence),
        6 => Ok(CancellationReason::SupervisorShutdown),
        7 => Ok(CancellationReason::BackendFailure),
        _ => Err(corrupt("terminal result has an unknown cancellation reason")),
    }
}

pub(super) const fn recovery_tag(value: TerminalRecovery) -> u8 {
    match value {
        TerminalRecovery::OriginalOwner => 1,
        TerminalRecovery::ReopenedTerminal => 2,
        TerminalRecovery::ReconciledLive => 3,
        TerminalRecovery::Indeterminate => 4,
    }
}

pub(super) const fn decode_recovery(tag: u8) -> Result<TerminalRecovery, ProcessError> {
    match tag {
        1 => Ok(TerminalRecovery::OriginalOwner),
        2 => Ok(TerminalRecovery::ReopenedTerminal),
        3 => Ok(TerminalRecovery::ReconciledLive),
        4 => Ok(TerminalRecovery::Indeterminate),
        _ => Err(corrupt("terminal result has an unknown recovery relation")),
    }
}
