//! Canonical version-one execution-plan encoding.

use peritus_types::RevisionTuple;

use crate::{
    BackendResourceFidelity, DeadlinePolicy, EnvironmentPlan, EnvironmentSource,
    EnvironmentValueSource, ExecutionIdentity, ExecutionIsolation, ExecutionPlan, GracefulAction,
    IoMode, OutputOverflowAction, OutputPolicy, ProcessError, ProcessResourcePolicy, StdinPolicy,
    TerminalSize, WorkspaceAccess, error::invalid,
};

const DOMAIN: &[u8] = b"peritus.execution-plan.v1\0";

pub(crate) fn encode(plan: &ExecutionPlan) -> Result<Vec<u8>, ProcessError> {
    let mut writer = PlanWriter::new();
    writer.raw(DOMAIN);
    let identity = plan.identity();
    encode_identity(&mut writer, &identity);
    writer.string(plan.command().executable())?;
    writer.length(plan.command().arguments().len())?;
    for argument in plan.command().arguments() {
        writer.string(argument)?;
    }
    writer.string(
        plan.working_directory()
            .path()
            .to_str()
            .ok_or_else(|| invalid("working directory became noncanonical"))?,
    )?;
    writer.u8(access_tag(plan.working_directory().access()));
    encode_environment(&mut writer, plan.environment())?;
    encode_io(&mut writer, plan.io_mode());
    encode_stdin(&mut writer, plan.stdin_policy());
    let terminal = plan.terminal_capabilities();
    writer.u8(u8::from(terminal.resize_allowed()));
    writer.u8(u8::from(terminal.signals_allowed()));
    writer.u64(terminal.event_count());
    writer.u64(terminal.output_bytes());
    encode_output(&mut writer, plan.output_policy());
    encode_deadlines(&mut writer, plan.deadline_policy());
    encode_resources(&mut writer, plan.resource_policy());
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
) -> Result<(), ProcessError> {
    match environment.source() {
        EnvironmentSource::Cleared => writer.u8(1),
        EnvironmentSource::Allowlisted(names) => {
            writer.u8(2);
            writer.length(names.len())?;
            for name in names {
                writer.string(name)?;
            }
        }
    }
    writer.length(environment.variables().len())?;
    for variable in environment.variables() {
        writer.u8(match variable.source() {
            EnvironmentValueSource::Inherited => 1,
            EnvironmentValueSource::Literal => 2,
        });
        writer.string(variable.name())?;
        writer.string(variable.value())?;
    }
    Ok(())
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

fn encode_stdin(writer: &mut PlanWriter, policy: StdinPolicy) {
    match policy {
        StdinPolicy::Closed => writer.u8(1),
        StdinPolicy::Bounded { max_write_bytes, max_total_bytes } => {
            writer.u8(2);
            writer.u64(max_write_bytes);
            writer.u64(max_total_bytes);
        }
    }
}

fn encode_output(writer: &mut PlanWriter, policy: OutputPolicy) {
    writer.u64(policy.chunk_bytes());
    writer.u64(policy.retained_window_bytes());
    writer.u64(policy.spool_bytes());
    writer.u64(policy.event_count());
    writer.u64(policy.stdout_bytes());
    writer.u64(policy.stderr_bytes());
    writer.u64(policy.terminal_bytes());
    writer.u8(match policy.overflow_action() {
        OutputOverflowAction::ContinueIncomplete => 1,
        OutputOverflowAction::Terminate => 2,
    });
}

fn encode_deadlines(writer: &mut PlanWriter, policy: DeadlinePolicy) {
    match policy.wall_timeout_millis() {
        Some(value) => {
            writer.u8(1);
            writer.u64(value);
        }
        None => writer.u8(0),
    }
    writer.u8(match policy.graceful_action() {
        GracefulAction::CloseInput => 1,
        GracefulAction::Interrupt => 2,
        GracefulAction::Terminate => 3,
    });
    writer.u64(policy.grace_millis());
    writer.u64(policy.reap_millis());
}

fn encode_resources(writer: &mut PlanWriter, policy: ProcessResourcePolicy) {
    writer.u64(policy.wall_millis());
    writer.u64(policy.cpu_millis());
    writer.u64(policy.memory_bytes());
    writer.u64(policy.disk_bytes());
    writer.u64(policy.output_bytes());
    writer.u64(policy.process_count());
    writer.u64(policy.file_descriptors());
    writer.u64(policy.concurrent_slots());
}

const fn access_tag(access: WorkspaceAccess) -> u8 {
    match access {
        WorkspaceAccess::ReadOnly => 1,
        WorkspaceAccess::Writable => 2,
    }
}

struct PlanWriter {
    bytes: Vec<u8>,
}

impl PlanWriter {
    fn new() -> Self {
        Self { bytes: Vec::with_capacity(1_024) }
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
        let value =
            u32::try_from(value).map_err(|_| invalid("canonical collection is too large"))?;
        self.raw(&value.to_be_bytes());
        Ok(())
    }
    fn string(&mut self, value: &str) -> Result<(), ProcessError> {
        self.length(value.len())?;
        self.raw(value.as_bytes());
        Ok(())
    }
}
