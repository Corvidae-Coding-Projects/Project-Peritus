//! Canonical version-one execution-plan encoding.

use peritus_types::RevisionTuple;

use crate::{
    BackendResourceFidelity, DeadlinePolicy, EnvironmentPlan, EnvironmentSource,
    EnvironmentValueSource, ExecutionIdentity, ExecutionIsolation, ExecutionPlan, GracefulAction,
    IoMode, OutputOverflowAction, OutputPolicy, ProcessError, ProcessResourcePolicy, StdinPolicy,
    TerminalSize, WorkspaceAccess, error::invalid,
};

const DOMAIN_V1: &[u8] = b"peritus.execution-plan.v1\0";
const DOMAIN_V2: &[u8] = b"peritus.execution-plan.v2\0";

pub(crate) fn encode(plan: &ExecutionPlan) -> Result<Vec<u8>, ProcessError> {
    let mut writer = PlanWriter::new();
    let output = plan.output_policy();
    let version_two = output.spool_bytes().is_none()
        || output.stdout_bytes().is_none()
        || output.stderr_bytes().is_none()
        || output.terminal_bytes().is_none()
        || plan.working_directory().path().to_str().is_none()
        || plan.resource_policy().wall_millis().is_none()
        || plan.resource_policy().cpu_millis().is_none()
        || plan.resource_policy().output_bytes().is_none();
    writer.raw(if version_two { DOMAIN_V2 } else { DOMAIN_V1 });
    let identity = plan.identity();
    encode_identity(&mut writer, &identity);
    writer.string(plan.command().executable())?;
    writer.length(plan.command().arguments().len())?;
    for argument in plan.command().arguments() {
        writer.string(argument)?;
    }
    writer.path(plan.working_directory().path(), version_two)?;
    writer.u8(access_tag(plan.working_directory().access()));
    encode_environment(&mut writer, plan.environment())?;
    encode_io(&mut writer, plan.io_mode());
    encode_stdin(&mut writer, plan.stdin_policy());
    let terminal = plan.terminal_capabilities();
    writer.u8(u8::from(terminal.resize_allowed()));
    writer.u8(u8::from(terminal.signals_allowed()));
    writer.u64(terminal.event_count());
    writer.u64(terminal.output_bytes());
    encode_output(&mut writer, output, version_two);
    encode_deadlines(&mut writer, plan.deadline_policy());
    encode_resources(&mut writer, plan.resource_policy(), version_two);
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

fn encode_output(writer: &mut PlanWriter, policy: OutputPolicy, version_two: bool) {
    writer.u64(policy.chunk_bytes());
    writer.u64(policy.retained_window_bytes());
    if version_two {
        writer.optional_u64(policy.spool_bytes());
    } else {
        writer.u64(policy.spool_bytes().expect("v1 requires a configured spool ceiling"));
    }
    writer.u64(policy.event_count());
    for limit in [policy.stdout_bytes(), policy.stderr_bytes(), policy.terminal_bytes()] {
        if version_two {
            writer.optional_u64(limit);
        } else {
            writer.u64(limit.expect("v1 requires configured stream ceilings"));
        }
    }
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

fn encode_resources(writer: &mut PlanWriter, policy: ProcessResourcePolicy, version_two: bool) {
    if version_two {
        writer.optional_u64(policy.wall_millis());
    } else {
        writer.u64(policy.wall_millis().unwrap_or_default());
    }
    if version_two {
        writer.optional_u64(policy.cpu_millis());
    } else {
        writer.u64(policy.cpu_millis().expect("v1 requires a configured CPU ceiling"));
    }
    writer.u64(policy.memory_bytes());
    writer.u64(policy.disk_bytes());
    if version_two {
        writer.optional_u64(policy.output_bytes());
    } else {
        writer.u64(policy.output_bytes().unwrap_or_default());
    }
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
    fn optional_u64(&mut self, value: Option<u64>) {
        if let Some(value) = value {
            self.u8(1);
            self.u64(value);
        } else {
            self.u8(0);
        }
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

    fn path(&mut self, value: &std::path::Path, version_two: bool) -> Result<(), ProcessError> {
        if let Some(text) = value.to_str() {
            if version_two {
                self.u8(0);
            }
            return self.string(text);
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            self.u8(1);
            let bytes = value.as_os_str().as_bytes();
            self.length(bytes.len())?;
            self.raw(bytes);
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt as _;
            self.u8(2);
            let units: Vec<u16> = value.as_os_str().encode_wide().collect();
            self.length(units.len())?;
            for unit in units {
                self.u16(unit);
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            return Err(invalid("working directory path has no exact canonical encoding"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DOMAIN_V1, PlanWriter, encode_output, encode_resources};
    use crate::{OutputOverflowAction, OutputPolicy, ProcessResourcePolicy};
    use std::path::Path;

    #[test]
    fn finite_v1_utf8_path_encoding_matches_legacy_golden_bytes() {
        let mut writer = PlanWriter::new();
        writer.path(Path::new("/workspace/project"), false).expect("v1 path");
        assert_eq!(writer.bytes, [18_u32.to_be_bytes().as_slice(), b"/workspace/project"].concat());
    }

    #[test]
    fn v2_utf8_path_encoding_has_an_explicit_portable_tag() {
        let mut writer = PlanWriter::new();
        writer.path(Path::new("/workspace/project"), true).expect("v2 path");
        assert_eq!(
            writer.bytes,
            [0_u8.to_be_bytes().as_slice(), 18_u32.to_be_bytes().as_slice(), b"/workspace/project"]
                .concat()
        );
    }

    #[test]
    fn finite_v1_output_and_resource_sections_match_legacy_golden_bytes() {
        let output =
            OutputPolicy::new(1, 2, 3, 4, 5, 6, 7, OutputOverflowAction::ContinueIncomplete)
                .expect("finite output policy");
        let resources =
            ProcessResourcePolicy::new(Some(11), Some(12), 13, 14, Some(15), 16, 17, 18)
                .expect("finite resource policy");
        let mut writer = PlanWriter::new();
        writer.raw(DOMAIN_V1);
        encode_output(&mut writer, output, false);
        encode_resources(&mut writer, resources, false);
        let mut golden = DOMAIN_V1.to_vec();
        for value in 1_u64..=7 {
            golden.extend_from_slice(&value.to_be_bytes());
        }
        golden.push(1);
        for value in 11_u64..=18 {
            golden.extend_from_slice(&value.to_be_bytes());
        }
        assert_eq!(writer.bytes, golden);
    }

    #[test]
    #[cfg(unix)]
    fn finite_v1_execution_plan_digest_matches_legacy_golden() {
        let plan = crate::plan::canonical_golden_fixture();
        assert!(plan.canonical_bytes().starts_with(DOMAIN_V1));
        assert_eq!(
            plan.digest().as_bytes(),
            &[
                0x69, 0x66, 0xf2, 0x65, 0x60, 0x20, 0x69, 0x19, 0x5b, 0xa9, 0x4d, 0x68, 0x31, 0xea,
                0x3e, 0xea, 0x21, 0x33, 0x10, 0xd2, 0xe8, 0xc7, 0x5e, 0xba, 0x50, 0xf7, 0x35, 0x86,
                0xc2, 0xb3, 0xcd, 0x78,
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_native_path_encoding_preserves_non_unicode_bytes() {
        use std::os::unix::ffi::OsStringExt as _;
        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 0xff]));
        let mut writer = PlanWriter::new();
        writer.path(&path, true).expect("native path");
        assert_eq!(
            writer.bytes,
            [
                1_u8.to_be_bytes().as_slice(),
                2_u32.to_be_bytes().as_slice(),
                [b'/', 0xff].as_slice()
            ]
            .concat()
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_native_path_encoding_preserves_utf16_units() {
        use std::os::windows::ffi::OsStringExt as _;
        let path =
            std::path::PathBuf::from(std::ffi::OsString::from_wide(&[u16::from(b'/'), 0xd800]));
        let mut writer = PlanWriter::new();
        writer.path(&path, true).expect("native path");
        assert_eq!(
            writer.bytes,
            [
                2_u8.to_be_bytes().as_slice(),
                2_u32.to_be_bytes().as_slice(),
                u16::from(b'/').to_be_bytes().as_slice(),
                0xd800_u16.to_be_bytes().as_slice()
            ]
            .concat()
        );
    }
}
