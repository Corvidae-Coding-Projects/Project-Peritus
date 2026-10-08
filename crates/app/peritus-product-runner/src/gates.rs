//! Candidate-aware D1 gate execution adapter.

use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::Read as _,
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(test)]
use std::process::Command;

use peritus_gates::{GateExecutionRecord, TargetGatePlan, TargetGateReport};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

mod artifact_csv;
mod deliverable_inventory;
mod explicit_paths;
mod json_structure;
mod source_layout;
mod sqlite_migration;
mod yaml_structure;

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, bundle::limit_text,
    control::PermissionCapability,
    developer_tools::{
        GateInvocationOutcome, GateInvocationRequest, WorkspaceOwnership,
    },
    execution::{CandidateRecorder, ProductDeliveryScope, ProductRunInput},
};

/// Rendered exact-target gate evidence and typed D1 report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateReport {
    pub report: TargetGateReport,
    pub output: String,
    pub(crate) execution_context: Sha256Digest,
}

impl GateReport {
    /// Host-observed inputs that can change how the exact gate plan executes.
    #[must_use]
    pub const fn execution_context(&self) -> Sha256Digest {
        self.execution_context
    }
}

pub(crate) enum GateRunOutcome {
    Complete(GateReport),
    Waiting { question: String },
    Superseded,
}

pub(crate) async fn run_with_ownership(
    input: &ProductRunInput,
    changed_paths: Vec<PathBuf>,
    ownership: &WorkspaceOwnership,
    delivery_scope: ProductDeliveryScope,
    transcript: &str,
    candidate: peritus_run_settlement::CandidateIdentity,
    recorder: &CandidateRecorder,
) -> Result<GateRunOutcome, ProductRunnerError> {
    if input.conversation.revision() != candidate.requirements_revision() {
        return Ok(GateRunOutcome::Superseded);
    }
    if !input.conversation.effective_permissions().allows(PermissionCapability::Read) {
        return Ok(read_permission_wait());
    }
    if !recorder.candidate_is_current(candidate, input.conversation.revision())? {
        return Ok(GateRunOutcome::Superseded);
    }
    let path_analysis = explicit_paths::analyze(&input.workspace_root, transcript);
    let plan = TargetGatePlan::discover(
        &input.workspace_root,
        changed_paths.clone(),
        &path_analysis.required_outputs(),
    )
    .map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Gate,
            "plan exact-target gates",
            error.to_string(),
        )
    })?;
    let external = plan.commands().iter().any(|command| command.program() != "peritus-internal");
    let network = plan.commands().iter().any(|command| {
        command.network_policy() == peritus_gates::GateNetworkPolicy::ExplicitAuthority
    });
    let permissions = input.conversation.effective_permissions();
    let mut missing = Vec::new();
    for (required, name) in [
        (external && !permissions.allows(crate::control::PermissionCapability::Write), "Write"),
        (
            external && !permissions.allows(crate::control::PermissionCapability::Process),
            "Process",
        ),
        (network && !permissions.allows(crate::control::PermissionCapability::Network), "Network"),
    ] {
        if required {
            missing.push(name);
        }
    }
    if !missing.is_empty() {
        return Ok(GateRunOutcome::Waiting {
            question: format!(
                "Exact-target verification is paused because {} permission is disabled. Restore {} in /permissions to continue this same candidate.",
                missing.join(" and "),
                missing.join(" and "),
            ),
        });
    }
    if input.conversation.revision() != candidate.requirements_revision() {
        return Ok(GateRunOutcome::Superseded);
    }
    if !input.conversation.effective_permissions().allows(PermissionCapability::Read) {
        return Ok(read_permission_wait());
    }
    let network_authorized = input
        .conversation
        .effective_permissions()
        .allows(PermissionCapability::Network);
    let base_context = execution_context(&input.workspace_root, &plan);
    let mut selected_commands = Vec::with_capacity(plan.commands().len());
    for command in plan.commands() {
        let grant = if command.program() == "peritus-internal" {
            None
        } else if let Some((workspace_id, catalog)) = input.command_runtime.managed_gate_network() {
            if workspace_id != candidate.workspace_id() {
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "select managed gate network",
                    "command runtime workspace identity differs from the retained candidate",
                ));
            }
            catalog.resolve_command(
                workspace_id,
                command.base_identity(),
                command.program(),
                network_authorized,
            )?
        } else {
            None
        };
        if command.network_policy() == peritus_gates::GateNetworkPolicy::ExplicitAuthority
            && grant.is_none()
        {
            return Ok(GateRunOutcome::Waiting {
                question: format!(
                    "Exact-target verification for `{}` requires managed network access, but the trusted service host has no exact grant for this command. Configure an exact host grant and keep Network permission enabled to continue this same candidate.",
                    command.label(),
                ),
            });
        }
        let selected = grant.as_ref().map_or_else(
            || command.clone(),
            |grant| command.clone().with_managed_network(grant.digest()),
        );
        selected_commands.push((selected, grant));
    }
    let context = execution_context_for_commands(
        &input.workspace_root,
        selected_commands.iter().map(|(command, _)| command),
    );
    let explicit_paths = explicit_paths::run_analyzed(
        &input.workspace_root,
        &path_analysis,
        &changed_paths,
    );
    let deliverable_inventory = deliverable_inventory::run(&path_analysis, &changed_paths);
    let mut records = Vec::new();
    for (index, (command, network_grant)) in selected_commands.iter().enumerate() {
        crate::execution::check_cancelled(input)?;
        if input.conversation.revision() != candidate.requirements_revision() {
            return Ok(GateRunOutcome::Superseded);
        }
        if !input.conversation.effective_permissions().allows(PermissionCapability::Read) {
            return Ok(read_permission_wait());
        }
        if !recorder.candidate_is_current(candidate, input.conversation.revision())? {
            return Ok(GateRunOutcome::Superseded);
        }
        if command.program() == "peritus-internal" {
            records.push(run_internal(
                &input.workspace_root,
                &plan,
                command,
                Some(ownership),
            ));
            if input.conversation.revision() != candidate.requirements_revision() {
                return Ok(GateRunOutcome::Superseded);
            }
            if !input.conversation.effective_permissions().allows(PermissionCapability::Read) {
                return Ok(read_permission_wait());
            }
            if !recorder.candidate_is_current(candidate, input.conversation.revision())? {
                return Ok(GateRunOutcome::Superseded);
            }
            continue;
        }
        let operation_key = operation_key(candidate, base_context, index, command);
        let conversation = Arc::clone(&input.conversation);
        let network_required = command.network_policy().requires_authority();
        let requirements_revision = candidate.requirements_revision();
        let authority_live = Arc::new(move || {
            let permissions = conversation.effective_permissions();
            conversation.revision() == requirements_revision
                && permissions.allows(PermissionCapability::Read)
                && permissions.allows(PermissionCapability::Write)
                && permissions.allows(PermissionCapability::Process)
                && (!network_required || permissions.allows(PermissionCapability::Network))
        });
        let request = network_grant.as_ref().map_or_else(
            || {
                GateInvocationRequest::new(
                    operation_key,
                    command.program().to_owned(),
                    command.arguments().to_vec(),
                    input.workspace_root.join(command.current_dir()),
                    Arc::clone(&authority_live),
                    input.provider_cancellation.clone(),
                )
            },
            |grant| {
                GateInvocationRequest::new_managed(
                    operation_key,
                    command.program().to_owned(),
                    command.arguments().to_vec(),
                    input.workspace_root.join(command.current_dir()),
                    Arc::clone(&authority_live),
                    input.provider_cancellation.clone(),
                    grant.clone(),
                )
            },
        );
        let runtime = input.command_runtime.clone();
        let outcome = tokio::task::spawn_blocking(move || runtime.run_native_gate(request))
            .await
            .map_err(|error| {
                ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "join retained native gate owner",
                    error.to_string(),
                )
            })?
            .map_err(|error| {
                ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "run retained native gate",
                    error,
                )
            })?;
        crate::execution::check_cancelled(input)?;
        if input.conversation.revision() != candidate.requirements_revision() {
            return Ok(GateRunOutcome::Superseded);
        }
        let permissions = input.conversation.effective_permissions();
        if !permissions.allows(PermissionCapability::Read)
            || !permissions.allows(PermissionCapability::Write)
            || !permissions.allows(PermissionCapability::Process)
            || (network_required && !permissions.allows(PermissionCapability::Network))
        {
            return Ok(GateRunOutcome::Waiting {
                question: format!(
                    "Exact-target verification for `{}` reached an owned observation boundary after its live permission was revoked. Restore the required permission to consume the retained result on this same candidate.",
                    command.label(),
                ),
            });
        }
        if !recorder.candidate_is_current(candidate, input.conversation.revision())? {
            return Ok(GateRunOutcome::Superseded);
        }
        match outcome {
            GateInvocationOutcome::Complete { exit_code, preview } => {
                records.push(GateExecutionRecord {
                    command: command.display(),
                    label: command.label().to_owned(),
                    exit_code,
                    output: preview,
                });
            }
            GateInvocationOutcome::Pending { detail }
            | GateInvocationOutcome::Unavailable { detail }
            | GateInvocationOutcome::AuthorityRevoked { detail } => {
                return Ok(GateRunOutcome::Waiting {
                    question: format!(
                        "Exact-target verification for `{}` is retained on this candidate: {detail}. Restore the required local capability or authority, then continue this same run.",
                        command.label(),
                    ),
                });
            }
        }
    }
    if input.conversation.revision() != candidate.requirements_revision() {
        return Ok(GateRunOutcome::Superseded);
    }
    if !input.conversation.effective_permissions().allows(PermissionCapability::Read) {
        return Ok(read_permission_wait());
    }
    if !recorder.candidate_is_current(candidate, input.conversation.revision())? {
        return Ok(GateRunOutcome::Superseded);
    }
    let report = TargetGateReport::from_execution_with_constraints(
        &plan,
        records,
        vec![explicit_paths, deliverable_inventory],
    );
    let output = render(&report, delivery_scope);
    Ok(GateRunOutcome::Complete(GateReport { report, output, execution_context: context }))
}

fn read_permission_wait() -> GateRunOutcome {
    GateRunOutcome::Waiting {
        question: "Exact-target verification is paused because Read permission is disabled. Restore Read in /permissions to continue this same candidate."
            .to_owned(),
    }
}

fn operation_key(
    candidate: peritus_run_settlement::CandidateIdentity,
    execution_context: Sha256Digest,
    index: usize,
    command: &peritus_gates::GateCommandSpec,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-retained-target-gate-v2\0");
    hasher.update(candidate.run_id().as_bytes());
    hasher.update(candidate.workspace_id().as_bytes());
    hasher.update(candidate.content_digest().as_bytes());
    hasher.update(candidate.repository_digest().as_bytes());
    hasher.update(candidate.requirements_revision().to_le_bytes());
    hasher.update(u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(execution_context.as_bytes());
    hasher.update(command.base_identity().as_bytes());
    Sha256Digest::new(hasher.finalize().into())
}

#[cfg(test)]
fn run_scoped(
    root: &Path,
    changed_paths: Vec<PathBuf>,
    ownership: Option<&WorkspaceOwnership>,
    delivery_scope: ProductDeliveryScope,
    transcript: &str,
) -> Result<GateReport, ProductRunnerError> {
    let path_analysis = explicit_paths::analyze(root, transcript);
    let requested_artifacts = path_analysis.required_outputs();
    let explicit_paths = explicit_paths::run_analyzed(root, &path_analysis, &changed_paths);
    let deliverable_inventory = deliverable_inventory::run(&path_analysis, &changed_paths);
    let plan =
        TargetGatePlan::discover(root, changed_paths, &requested_artifacts).map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Gate,
                "plan exact-target gates",
                error.to_string(),
            )
        })?;
    let execution_context = execution_context(root, &plan);
    let mut records = Vec::new();
    for specification in plan.commands() {
        if specification.program() == "peritus-internal" {
            records.push(run_internal(root, &plan, specification, ownership));
            continue;
        }
        let mut command = Command::new(specification.program());
        command.args(specification.arguments()).current_dir(root.join(specification.current_dir()));
        if specification.program() == "cargo" {
            command.env("CARGO_BUILD_JOBS", "2");
        }
        let record = match command.output() {
            Ok(output) => GateExecutionRecord {
                command: specification.display(),
                label: specification.label().to_owned(),
                exit_code: output.status.code(),
                output: limit_text(
                    &format!(
                        "{}{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    ),
                    512 * 1024,
                ),
            },
            Err(error) => GateExecutionRecord {
                command: specification.display(),
                label: specification.label().to_owned(),
                exit_code: None,
                output: error.to_string(),
            },
        };
        records.push(record);
    }
    let report = TargetGateReport::from_execution_with_constraints(
        &plan,
        records,
        vec![explicit_paths, deliverable_inventory],
    );
    let output = render(&report, delivery_scope);
    Ok(GateReport { report, output, execution_context })
}

fn run_internal(
    root: &Path,
    plan: &TargetGatePlan,
    specification: &peritus_gates::GateCommandSpec,
    ownership: Option<&WorkspaceOwnership>,
) -> GateExecutionRecord {
    match specification.arguments().first().map(String::as_str) {
        Some("source-readability") => source_layout::run(
            root,
            specification.project().root(),
            plan.changed_paths(),
            specification.project().kind(),
            specification.display(),
            ownership,
        ),
        Some("artifact-csv-structure") => artifact_csv::run(
            root,
            specification.project().root(),
            plan.changed_paths(),
            specification.display(),
        ),
        Some("json-structure") => json_structure::run(
            root,
            specification.project().root(),
            plan.changed_paths(),
            specification.display(),
        ),
        Some("sqlite-migration") => sqlite_migration::run(
            &root.join(specification.current_dir()),
            specification.display(),
        ),
        Some("yaml-structure") => yaml_structure::run(
            root,
            specification.project().root(),
            plan.changed_paths(),
            specification.display(),
        ),
        _ => GateExecutionRecord {
            command: specification.display(),
            label: specification.label().to_owned(),
            exit_code: None,
            output: "unknown internal exact-target gate".to_owned(),
        },
    }
}

fn execution_context(root: &Path, plan: &TargetGatePlan) -> Sha256Digest {
    execution_context_for_commands(root, plan.commands().iter())
}

fn execution_context_for_commands<'a>(
    root: &Path,
    commands: impl IntoIterator<Item = &'a peritus_gates::GateCommandSpec>,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-gate-execution-context-v3\0");
    hash_bytes(&mut hasher, std::env::consts::OS.as_bytes());
    hash_bytes(&mut hasher, std::env::consts::ARCH.as_bytes());
    hash_native_os_str(&mut hasher, root.as_os_str());
    let mut environment = std::env::vars_os().collect::<Vec<_>>();
    environment.sort_by(|left, right| {
        compare_native_os_str(&left.0, &right.0)
            .then_with(|| compare_native_os_str(&left.1, &right.1))
    });
    for (name, value) in environment {
        hasher.update([0]);
        hash_native_os_str(&mut hasher, &name);
        hash_native_os_str(&mut hasher, &value);
    }
    for command in commands {
        hasher.update([1]);
        hash_bytes(&mut hasher, command.identity().as_bytes());
        hash_bytes(&mut hasher, command.display().as_bytes());
        hash_native_os_str(&mut hasher, command.current_dir().as_os_str());
        observe_program(&mut hasher, root, command.current_dir(), command.program());
    }
    Sha256Digest::new(hasher.finalize().into())
}

fn observe_program(hasher: &mut Sha256, root: &Path, current_dir: &Path, program: &str) {
    let working_directory = root.join(current_dir);
    let resolved = if Path::new(program).components().count() > 1 {
        Some(working_directory.join(program))
    } else {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .flat_map(|directory| {
                    executable_extensions(program).into_iter().map(move |extension| {
                        let mut name = OsString::from(program);
                        name.push(extension);
                        directory.join(name)
                    })
                })
                .map(|candidate| {
                    if candidate.is_absolute() {
                        candidate
                    } else {
                        working_directory.join(candidate)
                    }
                })
                .find(|candidate| candidate.is_file())
        })
    };
    let Some(path) = resolved else {
        hasher.update([0]);
        hash_bytes(hasher, program.as_bytes());
        return;
    };
    hasher.update([1]);
    hash_native_os_str(hasher, path.as_os_str());
    match File::open(&path) {
        Ok(mut file) => {
            hasher.update([1]);
            let mut bytes = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                match file.read(&mut buffer) {
                    Ok(0) => {
                        hasher.update([1]);
                        hasher.update(bytes.to_le_bytes());
                        break;
                    }
                    Ok(count) => {
                        hasher.update(&buffer[..count]);
                        bytes = bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
                    }
                    Err(error) => {
                        hasher.update([0]);
                        hash_bytes(hasher, error.to_string().as_bytes());
                        break;
                    }
                }
            }
        }
        Err(error) => {
            hasher.update([0]);
            hash_bytes(hasher, error.to_string().as_bytes());
        }
    }
}

fn executable_extensions(program: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};

        if Path::new(program).extension().is_some() {
            return vec![OsString::new()];
        }
        let value = std::env::var_os("PATHEXT")
            .unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
        let units = value.encode_wide().collect::<Vec<_>>();
        let mut extensions = units
            .split(|unit| *unit == u16::from(b';'))
            .filter(|extension| !extension.is_empty())
            .map(OsString::from_wide)
            .collect::<Vec<_>>();
        extensions.push(OsString::new());
        extensions
    }
    #[cfg(not(windows))]
    {
        let _ = program;
        vec![OsString::new()]
    }
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

fn hash_native_os_str(hasher: &mut Sha256, value: &OsStr) {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;

        hasher.update([1]);
        hash_bytes(hasher, value.as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;

        hasher.update([2]);
        let units = value.encode_wide().collect::<Vec<_>>();
        hasher.update(u64::try_from(units.len()).unwrap_or(u64::MAX).to_le_bytes());
        for unit in units {
            hasher.update(unit.to_be_bytes());
        }
    }
}

fn compare_native_os_str(left: &OsStr, right: &OsStr) -> std::cmp::Ordering {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;

        left.as_bytes().cmp(right.as_bytes())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;

        left.encode_wide().cmp(right.encode_wide())
    }
}

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
fn render(report: &TargetGateReport, delivery_scope: ProductDeliveryScope) -> String {
    let mut text = String::new();
    text.push_str(&format!("Exact candidate files ({}):\n", report.changed_paths().len()));
    if report.changed_paths().is_empty() {
        if delivery_scope.allows_external_effects() {
            text.push_str(
                "  [none: caller-authorized external effects are evaluated separately]\n",
            );
        } else {
            text.push_str("  [none: acceptance is refused]\n");
        }
    } else {
        for path in report.changed_paths() {
            text.push_str(&format!("  {}\n", path.display()));
        }
    }
    if !report.uncovered_paths().is_empty() {
        text.push_str("\nUncovered candidate files:\n");
        for path in report.uncovered_paths() {
            text.push_str(&format!("  {}\n", path.display()));
        }
    }
    for record in report.records() {
        text.push_str(&format!(
            "\n[{}]\n$ {}\n{}\nexit: {}\n",
            record.label,
            record.command,
            record.output,
            record.exit_code.map_or_else(|| "not started".to_owned(), |code| code.to_string()),
        ));
    }
    let exact_target_status =
        if report.changed_paths().is_empty() && delivery_scope.allows_external_effects() {
            "NOT APPLICABLE"
        } else if report.passed() {
            "PASS"
        } else {
            "FAIL"
        };
    text.push_str(&format!("\nExact-target acceptance: {exact_target_status}\n"));
    limit_text(&text, 1024 * 1024)
}

#[cfg(test)]
mod tests;
