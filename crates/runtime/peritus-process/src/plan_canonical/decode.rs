//! Exact readers for versioned execution-plan canonical bytes.

use std::{ffi::OsString, path::PathBuf};

use peritus_policy::ActorRole;
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, AttemptId, CapabilityName, EnvironmentId, Generation,
    HarnessId, PolicyId, ProcessId, ProjectId, ProviderProfileId, ResourceId, RevisionNumber,
    RevisionTuple, RunId, SessionId, Sha256Digest, TurnId, WorkspaceId,
};

use crate::{
    BackendResourceFidelity, CommandSpec, DeadlinePolicy, EnvironmentPlan, EnvironmentSource,
    EnvironmentValueSource, EnvironmentVariable, ExecutionCallerBinding, ExecutionCallerTarget,
    ExecutionIdentity, ExecutionIsolation, GracefulAction, IoMode, OutputOverflowAction,
    OutputPolicy, ProcessError, ProcessResourcePolicy, StdinPolicy, TerminalCapabilities,
    TerminalSize, WorkingDirectory, WorkspaceAccess, error::invalid,
};

use super::{DOMAIN_V1, DOMAIN_V2, DOMAIN_V3};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Version {
    V1,
    V2,
    V3,
}

pub(crate) struct RestoredBackend {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) native: bool,
    pub(crate) resource_fidelity: BackendResourceFidelity,
    pub(crate) descriptor_digest: Sha256Digest,
    pub(crate) support_digest: Sha256Digest,
    pub(crate) preparation_digest: Sha256Digest,
}

pub(crate) struct RestoredPlan {
    pub(crate) identity: ExecutionIdentity,
    pub(crate) command: CommandSpec,
    pub(crate) working_directory: WorkingDirectory,
    pub(crate) environment: EnvironmentPlan,
    pub(crate) io_mode: IoMode,
    pub(crate) stdin: StdinPolicy,
    pub(crate) terminal: TerminalCapabilities,
    pub(crate) output: OutputPolicy,
    pub(crate) deadlines: DeadlinePolicy,
    pub(crate) resources: ProcessResourcePolicy,
    pub(crate) caller_binding: Option<ExecutionCallerBinding>,
    pub(crate) isolation: ExecutionIsolation,
    pub(crate) sandbox_digest: Sha256Digest,
    pub(crate) backend: RestoredBackend,
}

pub(crate) fn restore(bytes: &[u8]) -> Result<RestoredPlan, ProcessError> {
    let (version, offset) = if bytes.starts_with(DOMAIN_V1) {
        (Version::V1, DOMAIN_V1.len())
    } else if bytes.starts_with(DOMAIN_V2) {
        (Version::V2, DOMAIN_V2.len())
    } else if bytes.starts_with(DOMAIN_V3) {
        (Version::V3, DOMAIN_V3.len())
    } else {
        return Err(invalid("execution plan canonical domain is unsupported"));
    };
    let mut reader = Reader::new(bytes, offset);
    if version == Version::V3 {
        reader.require_native_platform()?;
    }
    let identity = decode_identity(&mut reader)?;
    let (executable, arguments, directory) = if version == Version::V3 {
        let executable = reader.native_text()?;
        let arguments = reader.sequence(4, |reader| reader.native_text())?;
        let directory = PathBuf::from(reader.native_text()?);
        (executable, arguments, directory)
    } else {
        let executable = OsString::from(reader.text()?);
        let arguments = reader.sequence(4, |reader| Ok(OsString::from(reader.text()?)))?;
        let directory = PathBuf::from(reader.text()?);
        (executable, arguments, directory)
    };
    let command = CommandSpec::new(executable, arguments)?;
    let access = match reader.u8()? {
        1 => WorkspaceAccess::ReadOnly,
        2 => WorkspaceAccess::Writable,
        _ => return Err(invalid("working-directory access tag is invalid")),
    };
    let revision = identity.revision();
    let working_directory = WorkingDirectory::restore_canonical_path(
        directory,
        identity.workspace_id(),
        identity.resource_id(),
        identity.environment_id(),
        revision.workspace_generation(),
        revision.workspace_revision(),
        access,
    )?;
    let environment = decode_environment(&mut reader, version)?;
    let io_mode = decode_io(&mut reader)?;
    let stdin = decode_stdin(&mut reader, version)?;
    let terminal = TerminalCapabilities::new(
        decode_bool(reader.u8()?)?,
        decode_bool(reader.u8()?)?,
        reader.u64()?,
        if version == Version::V1 {
            Some(reader.u64()?)
        } else {
            reader.optional_u64()?
        },
    );
    let output = decode_output(&mut reader, version)?;
    let deadlines = decode_deadlines(&mut reader, version)?;
    let resources = decode_resources(&mut reader, version)?;
    let caller_binding = decode_caller_binding(&mut reader)?;
    let isolation = match reader.u8()? {
        1 => ExecutionIsolation::Restricted,
        2 => ExecutionIsolation::ExplicitRawEffect,
        _ => return Err(invalid("execution isolation tag is invalid")),
    };
    let sandbox_digest = Sha256Digest::new(reader.array()?);
    let backend = RestoredBackend {
        name: reader.text()?,
        version: reader.text()?,
        native: decode_bool(reader.u8()?)?,
        resource_fidelity: match reader.u8()? {
            1 => BackendResourceFidelity::Hard,
            2 => BackendResourceFidelity::Supervisor,
            3 => BackendResourceFidelity::Reference,
            _ => return Err(invalid("backend resource-fidelity tag is invalid")),
        },
        descriptor_digest: Sha256Digest::new(reader.array()?),
        support_digest: Sha256Digest::new(reader.array()?),
        preparation_digest: Sha256Digest::new(reader.array()?),
    };
    reader.finish()?;
    Ok(RestoredPlan {
        identity,
        command,
        working_directory,
        environment,
        io_mode,
        stdin,
        terminal,
        output,
        deadlines,
        resources,
        caller_binding,
        isolation,
        sandbox_digest,
        backend,
    })
}

fn decode_identity(reader: &mut Reader<'_>) -> Result<ExecutionIdentity, ProcessError> {
    let project = ProjectId::new(reader.array()?)
        .map_err(|_| invalid("execution project id is invalid"))?;
    let session = SessionId::new(reader.array()?)
        .map_err(|_| invalid("execution session id is invalid"))?;
    let run = RunId::new(reader.array()?).map_err(|_| invalid("execution run id is invalid"))?;
    let attempt = AttemptId::new(reader.array()?)
        .map_err(|_| invalid("execution attempt id is invalid"))?;
    let turn = TurnId::new(reader.array()?).map_err(|_| invalid("execution turn id is invalid"))?;
    let action = ActionId::new(reader.array()?)
        .map_err(|_| invalid("execution action id is invalid"))?;
    let process = ProcessId::new(reader.array()?)
        .map_err(|_| invalid("execution process id is invalid"))?;
    let workspace = WorkspaceId::new(reader.array()?)
        .map_err(|_| invalid("execution workspace id is invalid"))?;
    let resource = ResourceId::new(reader.array()?)
        .map_err(|_| invalid("execution resource id is invalid"))?;
    let environment = EnvironmentId::new(reader.array()?)
        .map_err(|_| invalid("execution environment id is invalid"))?;
    let actor = ActorId::new(reader.array()?)
        .map_err(|_| invalid("execution actor id is invalid"))?;
    Ok(ExecutionIdentity::new(
        project,
        session,
        run,
        attempt,
        turn,
        action,
        process,
        workspace,
        resource,
        environment,
        actor,
        decode_revision(reader)?,
    ))
}

fn decode_revision(reader: &mut Reader<'_>) -> Result<RevisionTuple, ProcessError> {
    let acceptance = AcceptanceSpecId::new(reader.array()?)
        .map_err(|_| invalid("execution acceptance id is invalid"))?;
    let harness = HarnessId::new(reader.array()?)
        .map_err(|_| invalid("execution harness id is invalid"))?;
    let workspace = WorkspaceId::new(reader.array()?)
        .map_err(|_| invalid("execution revision workspace id is invalid"))?;
    let generation = Generation::new(reader.u64()?)
        .map_err(|_| invalid("execution workspace generation is invalid"))?;
    let revision = RevisionNumber::new(reader.u64()?)
        .map_err(|_| invalid("execution workspace revision is invalid"))?;
    let policy = PolicyId::new(reader.array()?)
        .map_err(|_| invalid("execution policy id is invalid"))?;
    let provider = ProviderProfileId::new(reader.array()?)
        .map_err(|_| invalid("execution provider profile id is invalid"))?;
    Ok(RevisionTuple::new(
        acceptance, harness, workspace, generation, revision, policy, provider,
    ))
}

fn decode_environment(
    reader: &mut Reader<'_>,
    version: Version,
) -> Result<EnvironmentPlan, ProcessError> {
    let source = match reader.u8()? {
        1 => EnvironmentSource::Cleared,
        2 => EnvironmentSource::Allowlisted(if version == Version::V3 {
            reader.sequence(4, |reader| reader.native_text())?
        } else {
            reader.sequence(4, |reader| Ok(OsString::from(reader.text()?)))?
        }),
        _ => return Err(invalid("environment source tag is invalid")),
    };
    let variables = reader.sequence(9, |reader| {
        let source = match reader.u8()? {
            1 => EnvironmentValueSource::Inherited,
            2 => EnvironmentValueSource::Literal,
            _ => return Err(invalid("environment value-source tag is invalid")),
        };
        let (name, value) = if version == Version::V3 {
            (reader.native_text()?, reader.native_text()?)
        } else {
            (OsString::from(reader.text()?), OsString::from(reader.text()?))
        };
        EnvironmentVariable::restore(name, value, source)
    })?;
    EnvironmentPlan::restore(source, variables)
}

fn decode_io(reader: &mut Reader<'_>) -> Result<IoMode, ProcessError> {
    match reader.u8()? {
        1 => Ok(IoMode::Pipes),
        2 => Ok(IoMode::Pty(TerminalSize::new(
            reader.u16()?,
            reader.u16()?,
            reader.u16()?,
            reader.u16()?,
        )?)),
        _ => Err(invalid("process I/O tag is invalid")),
    }
}

fn decode_stdin(reader: &mut Reader<'_>, version: Version) -> Result<StdinPolicy, ProcessError> {
    match reader.u8()? {
        1 => Ok(StdinPolicy::Closed),
        2 => StdinPolicy::bounded(reader.u64()?, reader.u64()?),
        3 if version != Version::V1 => StdinPolicy::streaming(reader.u64()?),
        _ => Err(invalid("stdin policy tag is invalid")),
    }
}

fn decode_output(reader: &mut Reader<'_>, version: Version) -> Result<OutputPolicy, ProcessError> {
    let chunk = reader.u64()?;
    let retained = reader.u64()?;
    if version == Version::V1 {
        return OutputPolicy::new(
            chunk,
            retained,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
            decode_overflow(reader.u8()?)?,
        );
    }
    let segment = reader.u64()?;
    let events = reader.u64()?;
    let spool = reader.optional_u64()?;
    let stdout = reader.optional_u64()?;
    let stderr = reader.optional_u64()?;
    let terminal = reader.optional_u64()?;
    let overflow = decode_overflow(reader.u8()?)?;
    match (spool, stdout, stderr, terminal) {
        (Some(spool), Some(stdout), Some(stderr), Some(terminal)) if segment == spool => {
            OutputPolicy::new(
                chunk, retained, spool, events, stdout, stderr, terminal, overflow,
            )
        }
        (None, None, None, None) if overflow == OutputOverflowAction::ContinueIncomplete => {
            OutputPolicy::streaming(chunk, retained, segment, events)
        }
        _ => Err(invalid("output policy optional allowances are inconsistent")),
    }
}

fn decode_overflow(tag: u8) -> Result<OutputOverflowAction, ProcessError> {
    match tag {
        1 => Ok(OutputOverflowAction::ContinueIncomplete),
        2 => Ok(OutputOverflowAction::Terminate),
        _ => Err(invalid("output overflow tag is invalid")),
    }
}

fn decode_deadlines(
    reader: &mut Reader<'_>,
    version: Version,
) -> Result<DeadlinePolicy, ProcessError> {
    let wall = reader.optional_u64()?;
    if version == Version::V1 {
        let action = decode_graceful_action(reader.u8()?)?;
        return DeadlinePolicy::new(wall, action, reader.u64()?, reader.u64()?);
    }
    let strategy = reader.u8()?;
    match strategy {
        1 => {
            let reap = reader.optional_u64()?;
            if reap.is_some() {
                return Err(invalid("force-stop policy has synthetic reap patience"));
            }
            DeadlinePolicy::force(wall)
        }
        2 => {
            let action = decode_graceful_action(reader.u8()?)?;
            let grace = reader.u64()?;
            let reap = reader.optional_u64()?;
            DeadlinePolicy::graceful(wall, action, grace, reap)
        }
        _ => Err(invalid("stop strategy tag is invalid")),
    }
}

fn decode_graceful_action(tag: u8) -> Result<GracefulAction, ProcessError> {
    match tag {
        1 => Ok(GracefulAction::CloseInput),
        2 => Ok(GracefulAction::Interrupt),
        3 => Ok(GracefulAction::Terminate),
        _ => Err(invalid("graceful action tag is invalid")),
    }
}

fn decode_resources(
    reader: &mut Reader<'_>,
    version: Version,
) -> Result<ProcessResourcePolicy, ProcessError> {
    if version == Version::V1 {
        let wall = nonzero(reader.u64()?);
        let cpu = nonzero(reader.u64()?);
        return ProcessResourcePolicy::with_optional_time(
            wall,
            cpu,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
        );
    }
    ProcessResourcePolicy::with_optional_limits(
        reader.optional_u64()?,
        reader.optional_u64()?,
        reader.optional_u64()?,
        reader.optional_u64()?,
        reader.optional_u64()?,
        reader.optional_u64()?,
        reader.optional_u64()?,
        reader.u64()?,
    )
}

fn decode_caller_binding(
    reader: &mut Reader<'_>,
) -> Result<Option<ExecutionCallerBinding>, ProcessError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => {
            let action = ActionId::new(reader.array()?)
                .map_err(|_| invalid("caller action id is invalid"))?;
            let capability = CapabilityName::new(reader.text()?)
                .map_err(|_| invalid("caller capability name is invalid"))?;
            let descriptor = Sha256Digest::new(reader.array()?);
            let prepared = Sha256Digest::new(reader.array()?);
            let actor = ActorId::new(reader.array()?)
                .map_err(|_| invalid("caller actor id is invalid"))?;
            let role = decode_role(reader.u8()?)?;
            let environment = EnvironmentId::new(reader.array()?)
                .map_err(|_| invalid("caller environment id is invalid"))?;
            let resource = ResourceId::new(reader.array()?)
                .map_err(|_| invalid("caller resource id is invalid"))?;
            Ok(Some(ExecutionCallerBinding::new(
                action,
                capability,
                descriptor,
                prepared,
                ExecutionCallerTarget::new(actor, role, environment, resource),
            )))
        }
        _ => Err(invalid("caller binding presence tag is invalid")),
    }
}

fn decode_role(tag: u8) -> Result<ActorRole, ProcessError> {
    match tag {
        1 => Ok(ActorRole::Writer),
        2 => Ok(ActorRole::Fixer),
        3 => Ok(ActorRole::Reviewer),
        4 => Ok(ActorRole::Evaluator),
        5 => Ok(ActorRole::GateRunner),
        6 => Ok(ActorRole::Orchestrator),
        7 => Ok(ActorRole::EvolutionAgent),
        8 => Ok(ActorRole::HumanAuthority),
        9 => Ok(ActorRole::DaemonService),
        10 => Ok(ActorRole::ProviderToolWorker),
        11 => Ok(ActorRole::Plugin),
        _ => Err(invalid("caller role tag is invalid")),
    }
}

const fn nonzero(value: u64) -> Option<u64> {
    if value == 0 { None } else { Some(value) }
}

fn decode_bool(tag: u8) -> Result<bool, ProcessError> {
    match tag {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("canonical boolean tag is invalid")),
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8], position: usize) -> Self {
        Self { bytes, position }
    }

    fn finish(&self) -> Result<(), ProcessError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid("execution plan canonical bytes contain trailing data"))
        }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProcessError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| invalid("execution plan canonical position overflow"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| invalid("execution plan canonical bytes are truncated"))?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProcessError> {
        self.take(N)?
            .try_into()
            .map_err(|_| invalid("execution plan canonical fixed field is truncated"))
    }

    fn u8(&mut self) -> Result<u8, ProcessError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, ProcessError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, ProcessError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ProcessError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn optional_u64(&mut self) -> Result<Option<u64>, ProcessError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u64()?)),
            _ => Err(invalid("optional integer tag is invalid")),
        }
    }

    fn text(&mut self) -> Result<String, ProcessError> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| invalid("execution plan text length is unsupported"))?;
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_| invalid("execution plan text is not UTF-8"))?;
        let mut owned = String::new();
        owned
            .try_reserve(value.len())
            .map_err(|_| invalid("allocate execution plan text"))?;
        owned.push_str(value);
        Ok(owned)
    }

    fn sequence<T>(
        &mut self,
        minimum_item_bytes: usize,
        mut decode: impl FnMut(&mut Self) -> Result<T, ProcessError>,
    ) -> Result<Vec<T>, ProcessError> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| invalid("execution plan sequence count is unsupported"))?;
        if minimum_item_bytes == 0 || count > self.remaining() / minimum_item_bytes {
            return Err(invalid("execution plan sequence count exceeds its bytes"));
        }
        let mut values = Vec::new();
        values
            .try_reserve(count)
            .map_err(|_| invalid("allocate execution plan sequence"))?;
        for _ in 0..count {
            values.push(decode(self)?);
        }
        Ok(values)
    }

    fn require_native_platform(&mut self) -> Result<(), ProcessError> {
        let expected = if cfg!(unix) { 1 } else { 2 };
        if self.u8()? == expected {
            Ok(())
        } else {
            Err(invalid("execution plan native bytes target another platform"))
        }
    }

    #[cfg(unix)]
    fn native_text(&mut self) -> Result<OsString, ProcessError> {
        use std::os::unix::ffi::OsStringExt as _;

        let length = usize::try_from(self.u32()?)
            .map_err(|_| invalid("execution native text length is unsupported"))?;
        let value = self.take(length)?;
        let mut owned = Vec::new();
        owned
            .try_reserve(value.len())
            .map_err(|_| invalid("allocate execution native text"))?;
        owned.extend_from_slice(value);
        Ok(OsString::from_vec(owned))
    }

    #[cfg(windows)]
    fn native_text(&mut self) -> Result<OsString, ProcessError> {
        use std::os::windows::ffi::OsStringExt as _;

        let length = usize::try_from(self.u32()?)
            .map_err(|_| invalid("execution native text length is unsupported"))?;
        if length > self.remaining() / 2 {
            return Err(invalid("execution native text is truncated"));
        }
        let mut units = Vec::new();
        units
            .try_reserve(length)
            .map_err(|_| invalid("allocate execution native text"))?;
        for _ in 0..length {
            units.push(self.u16()?);
        }
        Ok(OsString::from_wide(&units))
    }
}
