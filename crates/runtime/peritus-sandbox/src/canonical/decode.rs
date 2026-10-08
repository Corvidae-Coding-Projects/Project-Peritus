//! Exact readers for versioned checked-sandbox canonical bytes.

use std::{ffi::OsString, net::IpAddr, path::PathBuf};

use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId,
    ProviderProfileId, ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, Sha256Digest,
    WorkspaceId,
};

use crate::{
    BrokeredHandleLabel, CheckedSandboxPlan, DescendantPolicy, DnsName, EnvironmentContract,
    EnvironmentMode, EnvironmentName, EnvironmentRequirements, FeatureSet, FileOperation,
    FileOperationSet, FileRequirement, FilesystemContract, FilesystemRule, HostMatcher,
    InputPermission, IsolationRequirement, NativeExecutionAuthority, NetworkContract, NetworkHost,
    NetworkRule, NetworkTarget, PathScope, PortRange, ProcessContract, ProcessRequirements,
    ResizePermission, ResourceLimits, RuleEffect, SandboxBinding, SandboxContract, SandboxError,
    SandboxOperationClass, SandboxPath, SandboxRequirements, SecretContract, SecretDelivery,
    SecretGrant, SecretReference, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode,
    TerminalModes, TerminalRequirements, TerminalSignalPermission, TerminalSize, Transport,
    TreeContainment, compile_sandbox,
};

use super::{PLAN_DOMAIN_V1, PLAN_DOMAIN_V2, PLAN_DOMAIN_V3, PLAN_DOMAIN_V4};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Version {
    V1,
    V2,
    V3,
    V4,
}

pub(super) fn restore_plan(bytes: Vec<u8>) -> Result<CheckedSandboxPlan, SandboxError> {
    let (version, offset) = if bytes.starts_with(PLAN_DOMAIN_V1) {
        (Version::V1, PLAN_DOMAIN_V1.len())
    } else if bytes.starts_with(PLAN_DOMAIN_V2) {
        (Version::V2, PLAN_DOMAIN_V2.len())
    } else if bytes.starts_with(PLAN_DOMAIN_V3) {
        (Version::V3, PLAN_DOMAIN_V3.len())
    } else if bytes.starts_with(PLAN_DOMAIN_V4) {
        (Version::V4, PLAN_DOMAIN_V4.len())
    } else {
        return Err(invalid("sandbox canonical domain is unsupported"));
    };
    let mut reader = Reader::new(&bytes, offset);
    let binding = decode_binding(&mut reader)?;
    let isolation = match reader.u8()? {
        0 => IsolationRequirement::Restricted,
        1 => IsolationRequirement::ExplicitRawEffect,
        _ => return Err(invalid("sandbox isolation tag is invalid")),
    };
    let operation_class = match reader.u8()? {
        0 => SandboxOperationClass::Execution,
        1 => SandboxOperationClass::RawEffect,
        _ => return Err(invalid("sandbox operation-class tag is invalid")),
    };
    let contract = decode_contract(&mut reader)?;
    let requirements = decode_requirements(&mut reader)?;
    let native_execution = if matches!(version, Version::V3 | Version::V4) {
        Some(decode_native_execution(&mut reader, version == Version::V4)?)
    } else {
        None
    };
    let encoded_features = reader.u64()?;
    if encoded_features & !FeatureSet::all().bits() != 0 {
        return Err(invalid("sandbox canonical feature set contains unknown bits"));
    }
    reader.finish()?;

    let mut plan = compile_sandbox(binding, isolation, operation_class, contract, requirements)?;
    if let Some(authority) = native_execution {
        plan = plan.bind_native_execution(authority)?;
    }
    if plan.required_features().bits() != encoded_features || plan.canonical_bytes() != bytes {
        return Err(invalid("sandbox canonical bytes are not the exact checked representation"));
    }
    Ok(plan)
}

fn decode_binding(reader: &mut Reader<'_>) -> Result<SandboxBinding, SandboxError> {
    Ok(SandboxBinding::new(
        ProcessId::new(reader.array()?).map_err(|_| invalid("sandbox process id is invalid"))?,
        ResourceId::new(reader.array()?).map_err(|_| invalid("sandbox resource id is invalid"))?,
        EnvironmentId::new(reader.array()?)
            .map_err(|_| invalid("sandbox environment id is invalid"))?,
        decode_revision(reader)?,
    ))
}

fn decode_revision(reader: &mut Reader<'_>) -> Result<RevisionTuple, SandboxError> {
    let acceptance = AcceptanceSpecId::new(reader.array()?)
        .map_err(|_| invalid("sandbox acceptance id is invalid"))?;
    let harness = HarnessId::new(reader.array()?)
        .map_err(|_| invalid("sandbox harness id is invalid"))?;
    let workspace = WorkspaceId::new(reader.array()?)
        .map_err(|_| invalid("sandbox workspace id is invalid"))?;
    let generation = Generation::new(reader.u64()?)
        .map_err(|_| invalid("sandbox workspace generation is invalid"))?;
    let revision = RevisionNumber::new(reader.u64()?)
        .map_err(|_| invalid("sandbox workspace revision is invalid"))?;
    let policy = PolicyId::new(reader.array()?)
        .map_err(|_| invalid("sandbox policy id is invalid"))?;
    let provider = ProviderProfileId::new(reader.array()?)
        .map_err(|_| invalid("sandbox provider profile id is invalid"))?;
    Ok(RevisionTuple::new(
        acceptance, harness, workspace, generation, revision, policy, provider,
    ))
}

fn decode_contract(reader: &mut Reader<'_>) -> Result<SandboxContract, SandboxError> {
    let rules = reader.sequence(8, |reader| {
        let effect = decode_effect(reader.u8()?)?;
        let path = SandboxPath::new(reader.text()?)?;
        let scope = match reader.u8()? {
            0 => PathScope::Exact,
            1 => PathScope::Descendants,
            _ => return Err(invalid("filesystem scope tag is invalid")),
        };
        let bits = reader.u8()?;
        if bits == 0 || bits & !0x7f != 0 {
            return Err(invalid("filesystem operation set is invalid"));
        }
        let operations = FileOperationSet::from_operations(
            FileOperation::ALL
                .into_iter()
                .filter(|operation| bits & (1_u8 << operation_index(*operation)) != 0),
        );
        FilesystemRule::new(effect, path, scope, operations)
    })?;
    let filesystem = FilesystemContract::new(rules)?;
    let process = decode_process_contract(reader)?;
    let environment = decode_environment_contract(reader)?;
    let network = NetworkContract::new(reader.sequence(10, decode_network_rule)?)?;
    let secrets = SecretContract::new(reader.sequence(49, decode_secret_grant)?)?;
    let resources = decode_resource_limits(reader)?;
    let terminal = decode_terminal_contract(reader)?;
    Ok(SandboxContract::new(
        filesystem, process, environment, network, secrets, resources, terminal,
    ))
}

fn decode_process_contract(reader: &mut Reader<'_>) -> Result<ProcessContract, SandboxError> {
    let roots = reader.sequence(5, |reader| SandboxPath::new(reader.text()?))?;
    let descendants = match reader.u8()? {
        0 => DescendantPolicy::Denied,
        1 => DescendantPolicy::Bounded(reader.u32()?),
        2 => DescendantPolicy::Allowed,
        _ => return Err(invalid("descendant policy tag is invalid")),
    };
    let signals = match reader.u8()? {
        0 => SignalPolicy::Denied,
        1 => SignalPolicy::GracefulOnly,
        2 => SignalPolicy::GracefulAndForced,
        _ => return Err(invalid("process signal policy tag is invalid")),
    };
    let containment = match reader.u8()? {
        0 => TreeContainment::Required,
        1 => TreeContainment::NotRequiredForRawEffect,
        _ => return Err(invalid("process containment tag is invalid")),
    };
    let maximum = reader.u32()?;
    ProcessContract::with_optional_process_limit(
        roots,
        descendants,
        signals,
        containment,
        (maximum != 0).then_some(maximum),
    )
}

fn decode_environment_contract(
    reader: &mut Reader<'_>,
) -> Result<EnvironmentContract, SandboxError> {
    let mode = match reader.u8()? {
        0 => EnvironmentMode::Cleared,
        1 => EnvironmentMode::AllowListed(
            reader.sequence(5, |reader| EnvironmentName::new(reader.text()?))?,
        ),
        _ => return Err(invalid("environment mode tag is invalid")),
    };
    let literals = reader.sequence(5, |reader| EnvironmentName::new(reader.text()?))?;
    EnvironmentContract::new(mode, literals)
}

fn decode_network_rule(reader: &mut Reader<'_>) -> Result<NetworkRule, SandboxError> {
    let effect = decode_effect(reader.u8()?)?;
    let host = match reader.u8()? {
        0 => HostMatcher::DnsExact(DnsName::new(reader.text()?)?),
        1 => HostMatcher::DnsSuffix(DnsName::new(reader.text()?)?),
        2 => HostMatcher::ip_prefix(decode_ip(reader)?, reader.u8()?)?,
        _ => return Err(invalid("network host matcher tag is invalid")),
    };
    let transport = decode_transport(reader.u8()?)?;
    let ports = PortRange::new(reader.u16()?, reader.u16()?)?;
    Ok(NetworkRule::new(effect, host, transport, ports))
}

fn decode_secret_grant(reader: &mut Reader<'_>) -> Result<SecretGrant, SandboxError> {
    let reference = SecretReference::new(
        ResourceId::new(reader.array()?)
            .map_err(|_| invalid("secret resource id is invalid"))?,
        Sha256Digest::new(reader.array()?),
    );
    let delivery = match reader.u8()? {
        0 => SecretDelivery::Environment(EnvironmentName::new(reader.text()?)?),
        1 => SecretDelivery::File(SandboxPath::new(reader.text()?)?),
        2 => SecretDelivery::BrokeredHandle(BrokeredHandleLabel::new(reader.text()?)?),
        _ => return Err(invalid("secret delivery tag is invalid")),
    };
    Ok(SecretGrant::new(reference, delivery))
}

fn decode_resource_limits(reader: &mut Reader<'_>) -> Result<ResourceLimits, SandboxError> {
    let values = [
        reader.u64()?, reader.u64()?, reader.u64()?, reader.u64()?, reader.u64()?,
        reader.u64()?, reader.u64()?, reader.u64()?,
    ];
    ResourceLimits::with_optional_limits(
        quantity(values[0]),
        quantity(values[1]),
        quantity(values[2]),
        quantity(values[3]),
        quantity(values[4]),
        quantity(values[5]),
        quantity(values[6]),
        quantity(values[7]),
    )
}

fn decode_terminal_contract(reader: &mut Reader<'_>) -> Result<TerminalContract, SandboxError> {
    let modes = decode_modes(reader.u8()?)?;
    let input = decode_input(reader.u8()?)?;
    let resize = decode_resize(reader.u8()?)?;
    let signals = decode_terminal_signals(reader.u8()?)?;
    let maximum = decode_optional_terminal_size(reader)?;
    let events = ResourceQuantity::new(reader.u64()?);
    let output = quantity(reader.u64()?);
    let limits = TerminalLimits::with_optional_output(maximum, events, output)?;
    TerminalContract::new(modes, input, resize, signals, limits)
}

fn decode_requirements(reader: &mut Reader<'_>) -> Result<SandboxRequirements, SandboxError> {
    let files = reader.sequence(6, |reader| {
        let path = SandboxPath::new(reader.text()?)?;
        let operation = decode_file_operation(reader.u8()?)?;
        Ok(FileRequirement::new(path, operation))
    })?;
    let process = ProcessRequirements::new(
        SandboxPath::new(reader.text()?)?,
        reader.u32()?,
        decode_bool(reader.u8()?)?,
    );
    let inherited = reader.sequence(5, |reader| EnvironmentName::new(reader.text()?))?;
    let literals = reader.sequence(5, |reader| EnvironmentName::new(reader.text()?))?;
    let environment = EnvironmentRequirements::new(inherited, literals)?;
    let network = reader.sequence(8, decode_network_target)?;
    let secrets = reader.sequence(49, decode_secret_grant)?;
    let resources = decode_resource_limits(reader)?;
    let terminal = decode_terminal_requirements(reader)?;
    SandboxRequirements::new(
        files, process, environment, network, secrets, resources, terminal,
    )
}

fn decode_network_target(reader: &mut Reader<'_>) -> Result<NetworkTarget, SandboxError> {
    let host = match reader.u8()? {
        0 => NetworkHost::Dns(DnsName::new(reader.text()?)?),
        1 => NetworkHost::Ip(decode_ip(reader)?),
        _ => return Err(invalid("network target host tag is invalid")),
    };
    NetworkTarget::new(host, decode_transport(reader.u8()?)?, reader.u16()?)
}

fn decode_terminal_requirements(
    reader: &mut Reader<'_>,
) -> Result<TerminalRequirements, SandboxError> {
    let mode = match reader.u8()? {
        0 => TerminalMode::Pipes,
        1 => TerminalMode::Pty,
        _ => return Err(invalid("terminal mode tag is invalid")),
    };
    let input = decode_input(reader.u8()?)?;
    let resize = decode_resize(reader.u8()?)?;
    let signals = decode_terminal_signals(reader.u8()?)?;
    let size = decode_optional_terminal_size(reader)?;
    let events = ResourceQuantity::new(reader.u64()?);
    let output = quantity(reader.u64()?);
    TerminalRequirements::with_optional_output(
        mode, input, resize, signals, size, events, output,
    )
}

fn decode_native_execution(
    reader: &mut Reader<'_>,
    wide: bool,
) -> Result<NativeExecutionAuthority, SandboxError> {
    reader.require_native_platform()?;
    let executable = reader.native_text(wide)?;
    let directory = PathBuf::from(reader.native_text(wide)?);
    let minimum_item_bytes = if wide { 8 } else { 4 };
    let inherited =
        reader.native_sequence(wide, minimum_item_bytes, |reader| reader.native_text(wide))?;
    let literals =
        reader.native_sequence(wide, minimum_item_bytes, |reader| reader.native_text(wide))?;
    NativeExecutionAuthority::new(executable, directory, inherited, literals)
}

fn decode_optional_terminal_size(
    reader: &mut Reader<'_>,
) -> Result<Option<TerminalSize>, SandboxError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(TerminalSize::new(reader.u16()?, reader.u16()?)?)),
        _ => Err(invalid("optional terminal-size tag is invalid")),
    }
}

fn decode_modes(bits: u8) -> Result<TerminalModes, SandboxError> {
    if bits & !0x03 != 0 {
        return Err(invalid("terminal mode set contains unknown bits"));
    }
    Ok(TerminalModes::from_modes(
        [TerminalMode::Pipes, TerminalMode::Pty]
            .into_iter()
            .filter(|mode| match mode {
                TerminalMode::Pipes => bits & 1 != 0,
                TerminalMode::Pty => bits & 2 != 0,
            }),
    ))
}

fn decode_effect(tag: u8) -> Result<RuleEffect, SandboxError> {
    match tag {
        0 => Ok(RuleEffect::Allow),
        1 => Ok(RuleEffect::Deny),
        _ => Err(invalid("rule effect tag is invalid")),
    }
}

fn decode_transport(tag: u8) -> Result<Transport, SandboxError> {
    match tag {
        0 => Ok(Transport::Tcp),
        1 => Ok(Transport::Udp),
        _ => Err(invalid("network transport tag is invalid")),
    }
}

fn decode_file_operation(tag: u8) -> Result<FileOperation, SandboxError> {
    FileOperation::ALL
        .into_iter()
        .find(|operation| operation_index(*operation) == tag)
        .ok_or_else(|| invalid("filesystem operation tag is invalid"))
}

const fn operation_index(operation: FileOperation) -> u8 {
    match operation {
        FileOperation::Discover => 0,
        FileOperation::Metadata => 1,
        FileOperation::Read => 2,
        FileOperation::Execute => 3,
        FileOperation::Create => 4,
        FileOperation::Write => 5,
        FileOperation::Remove => 6,
    }
}

fn decode_input(tag: u8) -> Result<InputPermission, SandboxError> {
    match tag {
        0 => Ok(InputPermission::Denied),
        1 => Ok(InputPermission::Allowed),
        _ => Err(invalid("terminal input tag is invalid")),
    }
}

fn decode_resize(tag: u8) -> Result<ResizePermission, SandboxError> {
    match tag {
        0 => Ok(ResizePermission::Denied),
        1 => Ok(ResizePermission::Allowed),
        _ => Err(invalid("terminal resize tag is invalid")),
    }
}

fn decode_terminal_signals(tag: u8) -> Result<TerminalSignalPermission, SandboxError> {
    match tag {
        0 => Ok(TerminalSignalPermission::Denied),
        1 => Ok(TerminalSignalPermission::Allowed),
        _ => Err(invalid("terminal signal tag is invalid")),
    }
}

fn decode_bool(tag: u8) -> Result<bool, SandboxError> {
    match tag {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("sandbox canonical boolean is invalid")),
    }
}

fn decode_ip(reader: &mut Reader<'_>) -> Result<IpAddr, SandboxError> {
    reader.text()?.parse().map_err(|_| invalid("canonical IP address is invalid"))
}

const fn quantity(value: u64) -> Option<ResourceQuantity> {
    if value == 0 { None } else { Some(ResourceQuantity::new(value)) }
}

const fn invalid(detail: &'static str) -> SandboxError {
    crate::error::invalid(detail)
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8], position: usize) -> Self {
        Self { bytes, position }
    }

    fn finish(&self) -> Result<(), SandboxError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid("sandbox canonical bytes contain trailing data"))
        }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SandboxError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| invalid("sandbox canonical position overflow"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| invalid("sandbox canonical bytes are truncated"))?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], SandboxError> {
        self.take(N)?
            .try_into()
            .map_err(|_| invalid("sandbox canonical fixed field is truncated"))
    }

    fn u8(&mut self) -> Result<u8, SandboxError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, SandboxError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, SandboxError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, SandboxError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn text(&mut self) -> Result<String, SandboxError> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| invalid("sandbox canonical text length is unsupported"))?;
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_| invalid("sandbox canonical text is not UTF-8"))?;
        let mut owned = String::new();
        owned
            .try_reserve(value.len())
            .map_err(|_| invalid("allocate sandbox canonical text"))?;
        owned.push_str(value);
        Ok(owned)
    }

    fn sequence<T>(
        &mut self,
        minimum_item_bytes: usize,
        mut decode: impl FnMut(&mut Self) -> Result<T, SandboxError>,
    ) -> Result<Vec<T>, SandboxError> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| invalid("sandbox canonical sequence count is unsupported"))?;
        if minimum_item_bytes == 0 || count > self.remaining() / minimum_item_bytes {
            return Err(invalid("sandbox canonical sequence count exceeds its bytes"));
        }
        let mut values = Vec::new();
        values
            .try_reserve(count)
            .map_err(|_| invalid("allocate sandbox canonical sequence"))?;
        for _ in 0..count {
            values.push(decode(self)?);
        }
        Ok(values)
    }

    fn native_sequence<T>(
        &mut self,
        wide: bool,
        minimum_item_bytes: usize,
        mut decode: impl FnMut(&mut Self) -> Result<T, SandboxError>,
    ) -> Result<Vec<T>, SandboxError> {
        let count = self.native_length(wide)?;
        if minimum_item_bytes == 0 || count > self.remaining() / minimum_item_bytes {
            return Err(invalid("sandbox native sequence count exceeds its bytes"));
        }
        let mut values = Vec::new();
        values
            .try_reserve(count)
            .map_err(|_| invalid("allocate sandbox native sequence"))?;
        for _ in 0..count {
            values.push(decode(self)?);
        }
        Ok(values)
    }

    fn native_length(&mut self, wide: bool) -> Result<usize, SandboxError> {
        let value = if wide { self.u64()? } else { u64::from(self.u32()?) };
        usize::try_from(value)
            .map_err(|_| invalid("sandbox native length is unsupported on this platform"))
    }

    fn require_native_platform(&mut self) -> Result<(), SandboxError> {
        let expected = if cfg!(unix) { 1 } else { 2 };
        if self.u8()? == expected {
            Ok(())
        } else {
            Err(invalid("sandbox native canonical bytes target another platform"))
        }
    }

    #[cfg(unix)]
    fn native_text(&mut self, wide: bool) -> Result<OsString, SandboxError> {
        use std::os::unix::ffi::OsStringExt as _;

        let length = self.native_length(wide)?;
        let value = self.take(length)?;
        let mut owned = Vec::new();
        owned
            .try_reserve(value.len())
            .map_err(|_| invalid("allocate sandbox native text"))?;
        owned.extend_from_slice(value);
        Ok(OsString::from_vec(owned))
    }

    #[cfg(windows)]
    fn native_text(&mut self, wide: bool) -> Result<OsString, SandboxError> {
        use std::os::windows::ffi::OsStringExt as _;

        let length = self.native_length(wide)?;
        if length > self.remaining() / 2 {
            return Err(invalid("sandbox native text is truncated"));
        }
        let mut units = Vec::new();
        units
            .try_reserve(length)
            .map_err(|_| invalid("allocate sandbox native text"))?;
        for _ in 0..length {
            units.push(self.u16()?);
        }
        Ok(OsString::from_wide(&units))
    }
}
