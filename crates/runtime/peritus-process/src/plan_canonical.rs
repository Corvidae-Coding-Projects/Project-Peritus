//! Canonical version-one execution-plan encoding.

mod decode;

pub(crate) use decode::restore;

use peritus_types::RevisionTuple;

use crate::{
    BackendResourceFidelity, DeadlinePolicy, EnvironmentPlan, EnvironmentSource,
    EnvironmentValueSource, ExecutionIdentity, ExecutionIsolation, ExecutionPlan, GracefulAction,
    IoMode, OutputOverflowAction, OutputPolicy, ProcessError, ProcessResourcePolicy, StdinPolicy,
    StopStrategy, TerminalSize, WorkspaceAccess, error::invalid,
};

const DOMAIN_V1: &[u8] = b"peritus.execution-plan.v1\0";
const DOMAIN_V2: &[u8] = b"peritus.execution-plan.v2\0";
const DOMAIN_V3: &[u8] = b"peritus.execution-plan.v3\0";
const DOMAIN_V4: &[u8] = b"peritus.execution-plan.v4\0";

pub(crate) fn encode(plan: &ExecutionPlan) -> Result<Vec<u8>, ProcessError> {
    let wide = !fits_narrow_framing(plan);
    let legacy = !wide && uses_legacy_encoding(plan);
    let native_legacy = !wide && uses_legacy_native_encoding(plan);
    let mut writer = PlanWriter::new(wide);
    writer.raw(if wide {
        DOMAIN_V4
    } else if legacy {
        DOMAIN_V1
    } else if native_legacy {
        DOMAIN_V2
    } else {
        DOMAIN_V3
    });
    if !native_legacy {
        writer.u8(native_platform_tag());
    }
    let identity = plan.identity();
    encode_identity(&mut writer, &identity);
    if native_legacy {
        let (executable, arguments) = plan
            .command()
            .legacy_text()
            .ok_or_else(|| invalid("legacy command projection became unavailable"))?;
        writer.string(executable)?;
        writer.length(arguments.len())?;
        for argument in arguments {
            writer.string(argument)?;
        }
        writer.string(
            plan.working_directory()
                .path()
                .to_str()
                .ok_or_else(|| invalid("legacy working directory projection became unavailable"))?,
        )?;
    } else {
        writer.native_string(plan.command().executable())?;
        writer.length(plan.command().arguments().len())?;
        for argument in plan.command().arguments() {
            writer.native_string(argument)?;
        }
        writer.native_string(plan.working_directory().path().as_os_str())?;
    }
    writer.u8(access_tag(plan.working_directory().access()));
    encode_environment(&mut writer, plan.environment(), native_legacy)?;
    encode_io(&mut writer, plan.io_mode());
    encode_stdin(&mut writer, plan.stdin_policy(), legacy);
    let terminal = plan.terminal_capabilities();
    writer.u8(u8::from(terminal.resize_allowed()));
    writer.u8(u8::from(terminal.signals_allowed()));
    writer.u64(terminal.event_count());
    if legacy {
        writer.u64(terminal.output_bytes());
    } else {
        encode_optional_u64(&mut writer, terminal.output_limit());
    }
    encode_output(&mut writer, plan.output_policy(), legacy);
    encode_deadlines(&mut writer, plan.deadline_policy(), legacy);
    encode_resources(&mut writer, plan.resource_policy(), legacy);
    if let Some(binding) = plan.caller_binding() {
        writer.u8(1);
        writer.raw(binding.action_id().as_bytes());
        writer.string(binding.capability_name().as_str())?;
        writer.raw(binding.descriptor_digest().as_bytes());
        writer.raw(binding.prepared_digest().as_bytes());
        writer.raw(binding.actor_id().as_bytes());
        writer.u8(role_tag(binding.role()));
        writer.raw(binding.environment_id().as_bytes());
        writer.raw(binding.resource_id().as_bytes());
    } else {
        writer.u8(0);
    }
    writer.u8(match plan.isolation() {
        ExecutionIsolation::Restricted => 1,
        ExecutionIsolation::ExplicitRawEffect => 2,
    });
    writer.raw(plan.sandbox_digest().as_bytes());
    writer.string(plan.backend().name())?;
    writer.string(plan.backend().version())?;
    writer.u8(u8::from(plan.backend().is_native()));
    writer.u8(match plan.backend().resource_fidelity() {
        BackendResourceFidelity::Hard => 1,
        BackendResourceFidelity::Supervisor => 2,
        BackendResourceFidelity::Reference => 3,
    });
    writer.raw(plan.backend().descriptor_digest().as_bytes());
    writer.raw(plan.backend().support_digest().as_bytes());
    writer.raw(plan.backend().preparation_digest().as_bytes());
    Ok(writer.finish())
}

fn uses_legacy_encoding(plan: &ExecutionPlan) -> bool {
    uses_legacy_native_encoding(plan)
        && matches!(plan.stdin_policy(), StdinPolicy::Closed | StdinPolicy::Bounded { .. })
        && plan.output_policy().uses_legacy_encoding()
        && plan.deadline_policy().uses_legacy_encoding()
        && plan.resource_policy().has_selected_native_limits()
        && plan.terminal_capabilities().output_limit().is_some()
}

fn uses_legacy_native_encoding(plan: &ExecutionPlan) -> bool {
    plan.command().uses_legacy_encoding()
        && plan.working_directory().uses_legacy_encoding()
        && plan.environment().uses_legacy_encoding()
}

fn fits_narrow_framing(plan: &ExecutionPlan) -> bool {
    fits_u32(plan.command().arguments().len())
        && native_text_fits_u32(plan.command().executable())
        && plan.command().arguments().iter().all(|value| native_text_fits_u32(value))
        && native_text_fits_u32(plan.working_directory().path().as_os_str())
        && match plan.environment().source() {
            EnvironmentSource::Cleared => true,
            EnvironmentSource::Allowlisted(names) => {
                fits_u32(names.len()) && names.iter().all(|name| native_text_fits_u32(name))
            }
        }
        && fits_u32(plan.environment().variables().len())
        && plan.environment().variables().iter().all(|variable| {
            native_text_fits_u32(variable.name()) && native_text_fits_u32(variable.value())
        })
        && plan.caller_binding().is_none_or(|binding| {
            fits_u32(binding.capability_name().as_str().len())
        })
        && fits_u32(plan.backend().name().len())
        && fits_u32(plan.backend().version().len())
}

const fn fits_u32(value: usize) -> bool {
    value <= u32::MAX as usize
}

#[cfg(unix)]
fn native_text_fits_u32(value: &std::ffi::OsStr) -> bool {
    fits_u32(crate::command::native_bytes(value).len())
}

#[cfg(windows)]
fn native_text_fits_u32(value: &std::ffi::OsStr) -> bool {
    fits_u32(crate::command::native_units(value).count())
}

const fn role_tag(role: peritus_policy::ActorRole) -> u8 {
    match role {
        peritus_policy::ActorRole::Writer => 1,
        peritus_policy::ActorRole::Fixer => 2,
        peritus_policy::ActorRole::Reviewer => 3,
        peritus_policy::ActorRole::Evaluator => 4,
        peritus_policy::ActorRole::GateRunner => 5,
        peritus_policy::ActorRole::Orchestrator => 6,
        peritus_policy::ActorRole::EvolutionAgent => 7,
        peritus_policy::ActorRole::HumanAuthority => 8,
        peritus_policy::ActorRole::DaemonService => 9,
        peritus_policy::ActorRole::ProviderToolWorker => 10,
        peritus_policy::ActorRole::Plugin => 11,
    }
}

fn encode_identity(writer: &mut PlanWriter, identity: &ExecutionIdentity) {
    writer.raw(identity.project_id().as_bytes());
    writer.raw(identity.session_id().as_bytes());
    writer.raw(identity.run_id().as_bytes());
    writer.raw(identity.attempt_id().as_bytes());
    writer.raw(identity.turn_id().as_bytes());
    writer.raw(identity.action_id().as_bytes());
    writer.raw(identity.process_id().as_bytes());
    writer.raw(identity.workspace_id().as_bytes());
    writer.raw(identity.resource_id().as_bytes());
    writer.raw(identity.environment_id().as_bytes());
    writer.raw(identity.actor_id().as_bytes());
    encode_revision(writer, identity.revision());
}

fn encode_revision(writer: &mut PlanWriter, revision: RevisionTuple) {
    writer.raw(revision.acceptance_spec_id().as_bytes());
    writer.raw(revision.harness_id().as_bytes());
    writer.raw(revision.workspace_id().as_bytes());
    writer.u64(revision.workspace_generation().get());
    writer.u64(revision.workspace_revision().get());
    writer.raw(revision.policy_id().as_bytes());
    writer.raw(revision.provider_profile_id().as_bytes());
}

fn encode_environment(
    writer: &mut PlanWriter,
    environment: &EnvironmentPlan,
    legacy_native: bool,
) -> Result<(), ProcessError> {
    match environment.source() {
        EnvironmentSource::Cleared => writer.u8(1),
        EnvironmentSource::Allowlisted(names) => {
            writer.u8(2);
            writer.length(names.len())?;
            if legacy_native {
                let mut names = names
                    .iter()
                    .map(|name| {
                        name.to_str().ok_or_else(|| {
                            invalid("legacy environment name projection became unavailable")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                names.sort_by_key(|name| name.to_ascii_uppercase());
                for name in names {
                    writer.string(name)?;
                }
            } else {
                for name in names {
                    writer.native_string(name)?;
                }
            }
        }
    }
    writer.length(environment.variables().len())?;
    let mut variables = environment.variables().iter().collect::<Vec<_>>();
    if legacy_native {
        variables.sort_by_key(|variable| variable.name().to_str().map(str::to_ascii_uppercase));
    }
    for variable in variables {
        writer.u8(match variable.source() {
            EnvironmentValueSource::Inherited => 1,
            EnvironmentValueSource::Literal => 2,
        });
        if legacy_native {
            writer.string(
                variable
                    .name()
                    .to_str()
                    .ok_or_else(|| invalid("legacy environment name became unavailable"))?,
            )?;
            writer.string(
                variable
                    .value()
                    .to_str()
                    .ok_or_else(|| invalid("legacy environment value became unavailable"))?,
            )?;
        } else {
            writer.native_string(variable.name())?;
            writer.native_string(variable.value())?;
        }
    }
    Ok(())
}

const fn native_platform_tag() -> u8 {
    #[cfg(unix)]
    {
        1
    }
    #[cfg(windows)]
    {
        2
    }
}

fn encode_io(writer: &mut PlanWriter, mode: IoMode) {
    match mode {
        IoMode::Pipes => writer.u8(1),
        IoMode::Pty(size) => {
            writer.u8(2);
            encode_size(writer, size);
        }
    }
}

fn encode_size(writer: &mut PlanWriter, size: TerminalSize) {
    writer.u16(size.rows());
    writer.u16(size.columns());
    writer.u16(size.pixel_width());
    writer.u16(size.pixel_height());
}

fn encode_stdin(writer: &mut PlanWriter, policy: StdinPolicy, legacy: bool) {
    match policy {
        StdinPolicy::Closed => writer.u8(1),
        StdinPolicy::Bounded { max_write_bytes, max_total_bytes } => {
            writer.u8(2);
            writer.u64(max_write_bytes);
            writer.u64(max_total_bytes);
        }
        StdinPolicy::Streaming { max_write_bytes } => {
            debug_assert!(!legacy);
            writer.u8(3);
            writer.u64(max_write_bytes);
        }
    }
}

fn encode_output(writer: &mut PlanWriter, policy: OutputPolicy, legacy: bool) {
    writer.u64(policy.chunk_bytes());
    writer.u64(policy.retained_window_bytes());
    if legacy {
        writer.u64(policy.spool_bytes());
        writer.u64(policy.event_count());
        writer.u64(policy.stdout_bytes());
        writer.u64(policy.stderr_bytes());
        writer.u64(policy.terminal_bytes());
    } else {
        writer.u64(policy.spool_segment_bytes());
        writer.u64(policy.event_count());
        encode_optional_u64(writer, policy.spool_limit());
        encode_optional_u64(writer, policy.stdout_limit());
        encode_optional_u64(writer, policy.stderr_limit());
        encode_optional_u64(writer, policy.terminal_limit());
    }
    writer.u8(match policy.overflow_action() {
        OutputOverflowAction::ContinueIncomplete => 1,
        OutputOverflowAction::Terminate => 2,
    });
}

fn encode_deadlines(writer: &mut PlanWriter, policy: DeadlinePolicy, legacy: bool) {
    match policy.wall_timeout_millis() {
        Some(value) => {
            writer.u8(1);
            writer.u64(value);
        }
        None => writer.u8(0),
    }
    match policy.stop_strategy() {
        StopStrategy::Force => {
            debug_assert!(!legacy);
            writer.u8(1);
        }
        StopStrategy::GracefulThenForce { action, escalation_millis } => {
            writer.u8(if legacy {
                match action {
                    GracefulAction::CloseInput => 1,
                    GracefulAction::Interrupt => 2,
                    GracefulAction::Terminate => 3,
                }
            } else {
                2
            });
            if !legacy {
                writer.u8(match action {
                    GracefulAction::CloseInput => 1,
                    GracefulAction::Interrupt => 2,
                    GracefulAction::Terminate => 3,
                });
            }
            writer.u64(escalation_millis);
        }
    }
    if legacy {
        writer.u64(policy.reap_millis().expect("legacy deadline has selected reap patience"));
    } else {
        encode_optional_u64(writer, policy.reap_millis());
    }
}

fn encode_resources(writer: &mut PlanWriter, policy: ProcessResourcePolicy, legacy: bool) {
    if legacy {
        writer.u64(policy.wall_millis().unwrap_or(0));
        writer.u64(policy.cpu_millis().unwrap_or(0));
        writer.u64(policy.memory_bytes());
        writer.u64(policy.disk_bytes());
        writer.u64(policy.output_bytes());
        writer.u64(policy.process_count());
        writer.u64(policy.file_descriptors());
    } else {
        encode_optional_u64(writer, policy.wall_millis());
        encode_optional_u64(writer, policy.cpu_millis());
        encode_optional_u64(writer, policy.memory_limit());
        encode_optional_u64(writer, policy.disk_limit());
        encode_optional_u64(writer, policy.output_limit());
        encode_optional_u64(writer, policy.process_limit());
        encode_optional_u64(writer, policy.file_descriptor_limit());
    }
    writer.u64(policy.concurrent_slots());
}

fn encode_optional_u64(writer: &mut PlanWriter, value: Option<u64>) {
    match value {
        Some(value) => {
            writer.u8(1);
            writer.u64(value);
        }
        None => writer.u8(0),
    }
}

const fn access_tag(access: WorkspaceAccess) -> u8 {
    match access {
        WorkspaceAccess::ReadOnly => 1,
        WorkspaceAccess::Writable => 2,
    }
}

struct PlanWriter {
    bytes: Vec<u8>,
    wide: bool,
}

impl PlanWriter {
    fn new(wide: bool) -> Self {
        Self { bytes: Vec::with_capacity(1_024), wide }
    }
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
    fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.raw(&value.to_be_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }
    fn length(&mut self, value: usize) -> Result<(), ProcessError> {
        if self.wide {
            let value = u64::try_from(value)
                .map_err(|_| invalid("canonical length exceeds the native representation"))?;
            self.raw(&value.to_be_bytes());
        } else {
            let value =
                u32::try_from(value).map_err(|_| invalid("canonical collection is too large"))?;
            self.raw(&value.to_be_bytes());
        }
        Ok(())
    }
    fn string(&mut self, value: &str) -> Result<(), ProcessError> {
        self.length(value.len())?;
        self.raw(value.as_bytes());
        Ok(())
    }
    #[cfg(unix)]
    fn native_string(&mut self, value: &std::ffi::OsStr) -> Result<(), ProcessError> {
        let value = crate::command::native_bytes(value);
        self.length(value.len())?;
        self.raw(value);
        Ok(())
    }
    #[cfg(windows)]
    fn native_string(&mut self, value: &std::ffi::OsStr) -> Result<(), ProcessError> {
        let value = crate::command::native_units(value).collect::<Vec<_>>();
        self.length(value.len())?;
        for unit in value {
            self.raw(&unit.to_be_bytes());
        }
        Ok(())
    }
}
