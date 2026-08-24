//! Canonical manifest field codecs and validation.

use std::path::Path;

use peritus_sandbox::SandboxResourceKind;
use peritus_types::Sha256Digest;

use crate::{
    EnforcementLevel, EnvironmentEntry, MacosError, MacosOperation, ProcessContainment,
    ProxyHandleDescriptor, ResourceControl, ResourceControlPlan, SecretHandleDescriptor,
    TerminalMapping,
    canonical::{Reader, Writer},
    error,
    resource::{resource_from_ordinal, resource_ordinal},
};

use super::{MAX_ARGUMENTS, PREPARATION_DOMAIN};

pub(super) fn expected_preparation(
    plan: Sha256Digest,
    descriptor: Sha256Digest,
    support: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::from(PREPARATION_DOMAIN);
    bytes.extend_from_slice(plan.as_bytes());
    bytes.extend_from_slice(descriptor.as_bytes());
    bytes.extend_from_slice(support.as_bytes());
    peritus_codec::sha256(&bytes)
}

pub(super) fn path_text(path: &Path) -> Result<&str, MacosError> {
    path.to_str()
        .ok_or_else(|| error::invalid(MacosOperation::Manifest, "manifest path is not valid UTF-8"))
}

pub(super) fn validate_executable_path(path: &Path) -> Result<(), MacosError> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "native executable path must be absolute",
        ));
    }
    path_text(path)?;
    Ok(())
}

pub(super) fn validate_executable_text(value: &str) -> Result<(), MacosError> {
    if !value.starts_with('/') || value.is_empty() || value.as_bytes().contains(&0) {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "target executable must be a literal absolute path",
        ));
    }
    Ok(())
}

pub(super) fn validate_working_directory(path: &Path) -> Result<(), MacosError> {
    if !path.is_absolute() {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "target working directory must be absolute",
        ));
    }
    path_text(path)?;
    Ok(())
}

pub(super) fn encode_strings(writer: &mut Writer, values: &[String]) -> Result<(), MacosError> {
    if values.len() > MAX_ARGUMENTS {
        return Err(error::limited(MacosOperation::Manifest, "too many target arguments"));
    }
    writer.count(values.len())?;
    for value in values {
        writer.string(value)?;
    }
    Ok(())
}

pub(super) fn decode_strings(reader: &mut Reader<'_>) -> Result<Vec<String>, MacosError> {
    let count = reader.count()?;
    if count > MAX_ARGUMENTS {
        return Err(error::limited(MacosOperation::Manifest, "too many target arguments"));
    }
    (0..count).map(|_| reader.string()).collect()
}

pub(super) fn decode_environment(
    reader: &mut Reader<'_>,
) -> Result<Vec<EnvironmentEntry>, MacosError> {
    let count = reader.count()?;
    let mut environment =
        (0..count).map(|_| EnvironmentEntry::decode(reader)).collect::<Result<Vec<_>, _>>()?;
    crate::environment::canonicalize(&mut environment)?;
    Ok(environment)
}

pub(super) fn validate_control_environment(
    environment: &[EnvironmentEntry],
    proxy: Option<&ProxyHandleDescriptor>,
    secrets: &[SecretHandleDescriptor],
) -> Result<(), MacosError> {
    const RESERVED: [&str; 2] = ["PERITUS_NATIVE_PTY_SLAVE_V1", "PERITUS_NATIVE_SECRET_HANDLES_V1"];
    let collides = environment.iter().any(|entry| {
        RESERVED.iter().any(|name| entry.name().eq_ignore_ascii_case(name))
            || (proxy.is_some()
                && ["HTTP_PROXY", "HTTPS_PROXY"]
                    .iter()
                    .any(|name| entry.name().eq_ignore_ascii_case(name)))
            || secrets.iter().any(|secret| {
                matches!(
                    secret.destination(),
                    crate::SecretHandleDestination::Environment(name)
                        if entry.name().eq_ignore_ascii_case(name.as_str())
                )
            })
    });
    if collides {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "target environment collides with protected native delivery",
        ));
    }
    Ok(())
}

pub(super) fn encode_proxy(
    writer: &mut Writer,
    proxy: Option<&ProxyHandleDescriptor>,
) -> Result<(), MacosError> {
    writer.boolean(proxy.is_some())?;
    proxy.map_or(Ok(()), |proxy| proxy.encode(writer))
}

pub(super) fn decode_proxy(
    reader: &mut Reader<'_>,
) -> Result<Option<ProxyHandleDescriptor>, MacosError> {
    reader.boolean()?.then(|| ProxyHandleDescriptor::decode(reader)).transpose()
}

pub(super) fn encode_resources(
    writer: &mut Writer,
    plan: &ResourceControlPlan,
) -> Result<(), MacosError> {
    writer.count(plan.controls().len())?;
    for control in plan.controls() {
        writer.u8(resource_ordinal(control.kind()))?;
        writer.u64(control.ceiling())?;
        writer.u8(control.level().ordinal())?;
    }
    Ok(())
}

pub(super) fn decode_resources(reader: &mut Reader<'_>) -> Result<ResourceControlPlan, MacosError> {
    let count = reader.count()?;
    if count != 8 {
        return Err(error::invalid(MacosOperation::Manifest, "resource mapping is not complete"));
    }
    let mut controls = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = resource_from_ordinal(reader.u8()?).ok_or_else(|| {
            error::invalid(MacosOperation::Manifest, "invalid resource dimension")
        })?;
        let ceiling = reader.u64()?;
        let level = EnforcementLevel::from_ordinal(reader.u8()?).ok_or_else(|| {
            error::invalid(MacosOperation::Manifest, "invalid resource enforcement level")
        })?;
        controls.push(ResourceControl::new(kind, ceiling, level));
    }
    let expected = [
        SandboxResourceKind::WallTime,
        SandboxResourceKind::CpuTime,
        SandboxResourceKind::Memory,
        SandboxResourceKind::Disk,
        SandboxResourceKind::Output,
        SandboxResourceKind::OpenHandles,
        SandboxResourceKind::Processes,
        SandboxResourceKind::Concurrency,
    ];
    if !controls.iter().zip(expected).all(|(control, kind)| control.kind() == kind) {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "resource mapping is duplicated or out of order",
        ));
    }
    let controls = controls.try_into().map_err(|_| {
        error::invalid(MacosOperation::Manifest, "resource mapping is not complete")
    })?;
    Ok(ResourceControlPlan::from_controls(controls))
}

pub(super) fn encode_containment(
    writer: &mut Writer,
    containment: ProcessContainment,
) -> Result<(), MacosError> {
    writer.boolean(containment.new_process_group())?;
    writer.boolean(containment.tree_required())?;
    writer.u32(containment.descendant_limit())?;
    writer.boolean(containment.graceful_signal())?;
    writer.boolean(containment.forced_signal())
}

pub(super) fn decode_containment(
    reader: &mut Reader<'_>,
) -> Result<ProcessContainment, MacosError> {
    ProcessContainment::from_manifest(
        reader.boolean()?,
        reader.boolean()?,
        reader.u32()?,
        reader.boolean()?,
        reader.boolean()?,
    )
}

pub(super) fn encode_terminal(
    writer: &mut Writer,
    terminal: TerminalMapping,
) -> Result<(), MacosError> {
    match terminal {
        TerminalMapping::Pipes { input } => {
            writer.u8(0)?;
            writer.boolean(input)
        }
        TerminalMapping::Pty { columns, rows, resize, signals, input } => {
            writer.u8(1)?;
            writer.u16(columns)?;
            writer.u16(rows)?;
            writer.boolean(resize)?;
            writer.boolean(signals)?;
            writer.boolean(input)
        }
    }
}

pub(super) fn decode_terminal(reader: &mut Reader<'_>) -> Result<TerminalMapping, MacosError> {
    match reader.u8()? {
        0 => Ok(TerminalMapping::Pipes { input: reader.boolean()? }),
        1 => {
            let columns = reader.u16()?;
            let rows = reader.u16()?;
            if columns == 0 || rows == 0 {
                return Err(error::invalid(
                    MacosOperation::Manifest,
                    "PTY dimensions must be nonzero",
                ));
            }
            Ok(TerminalMapping::Pty {
                columns,
                rows,
                resize: reader.boolean()?,
                signals: reader.boolean()?,
                input: reader.boolean()?,
            })
        }
        _ => Err(error::invalid(MacosOperation::Manifest, "invalid terminal mode")),
    }
}

pub(super) fn decode_secrets(
    reader: &mut Reader<'_>,
) -> Result<Vec<SecretHandleDescriptor>, MacosError> {
    let count = reader.count()?;
    (0..count).map(|_| SecretHandleDescriptor::decode(reader)).collect()
}

pub(super) fn validate_protected_handles(
    exec_status_descriptor: u32,
    proxy: Option<&ProxyHandleDescriptor>,
    secrets: &[SecretHandleDescriptor],
) -> Result<(), MacosError> {
    if exec_status_descriptor < 3 || exec_status_descriptor > i32::MAX.cast_unsigned() {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "helper exec status descriptor is outside the native range",
        ));
    }
    if secrets.windows(2).any(|pair| {
        pair[0].descriptor() >= pair[1].descriptor() || pair[0].label() == pair[1].label()
    }) {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "secret handles are duplicated or out of canonical order",
        ));
    }
    for (index, secret) in secrets.iter().enumerate() {
        if secrets[..index].iter().any(|prior| prior.destination() == secret.destination()) {
            return Err(error::invalid(MacosOperation::Manifest, "secret destinations duplicate"));
        }
    }
    if proxy.is_some_and(|proxy| {
        secrets.iter().any(|secret| {
            secret.descriptor() == proxy.route().routing_handle() || secret.label() == proxy.label()
        }) || proxy.route().routing_handle() == exec_status_descriptor
    }) {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "proxy, secret, and helper status handles collide",
        ));
    }
    if secrets.iter().any(|secret| secret.descriptor() == exec_status_descriptor) {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "secret and helper status handles collide",
        ));
    }
    Ok(())
}
