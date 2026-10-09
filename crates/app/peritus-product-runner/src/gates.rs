//! Candidate-aware D1 gate execution adapter.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use peritus_gates::{GateExecutionRecord, GateObservation, TargetGatePlan, TargetGateReport};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

mod artifact_csv;
mod cancellation;
mod deliverable_inventory;
mod explicit_paths;
mod json_structure;
mod source_layout;
mod sqlite_migration;
mod yaml_structure;

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, bundle::limit_text,
    developer_tools::WorkspaceOwnership, execution::ProductDeliveryScope,
};

pub use cancellation::GateCancellation;

pub enum GateOutcome {
    Required(GateExecutionRecord),
    Optional(GateObservation),
}

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

pub async fn run_with_ownership(
    root: &Path,
    changed_paths: Vec<PathBuf>,
    ownership: &WorkspaceOwnership,
    delivery_scope: ProductDeliveryScope,
    transcript: &str,
    request_context: &str,
    cancellation: GateCancellation,
) -> Result<GateReport, ProductRunnerError> {
    let root = root.to_owned();
    let ownership = ownership.clone();
    let transcript = transcript.to_owned();
    let request_context = request_context.to_owned();
    tokio::task::spawn_blocking(move || {
        run_scoped_with_cancellation(
            &root,
            changed_paths,
            Some(&ownership),
            delivery_scope,
            &transcript,
            &request_context,
            &cancellation,
        )
    })
    .await
    .map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Gate,
            "run exact-target gates",
            format!("blocking gate worker failed: {error}"),
        )
    })?
}

#[cfg(test)]
fn run_scoped(
    root: &Path,
    changed_paths: Vec<PathBuf>,
    ownership: Option<&WorkspaceOwnership>,
    delivery_scope: ProductDeliveryScope,
    transcript: &str,
) -> Result<GateReport, ProductRunnerError> {
    run_scoped_with_cancellation(
        root,
        changed_paths,
        ownership,
        delivery_scope,
        transcript,
        "",
        &GateCancellation::default(),
    )
}

fn run_scoped_with_cancellation(
    root: &Path,
    changed_paths: Vec<PathBuf>,
    ownership: Option<&WorkspaceOwnership>,
    delivery_scope: ProductDeliveryScope,
    transcript: &str,
    request_context: &str,
    cancellation: &GateCancellation,
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
    let execution_context = execution_context(root, &plan, request_context);
    let (records, observations) =
        execute_plan(root, &plan, ownership, request_context, cancellation);
    let report = TargetGateReport::from_execution_with_observations(
        &plan,
        records,
        observations,
        vec![explicit_paths, deliverable_inventory],
    );
    let output = render(&report, delivery_scope);
    Ok(GateReport { report, output, execution_context })
}

fn execute_plan(
    root: &Path,
    plan: &TargetGatePlan,
    ownership: Option<&WorkspaceOwnership>,
    request_context: &str,
    cancellation: &GateCancellation,
) -> (Vec<GateExecutionRecord>, Vec<GateObservation>) {
    let mut records = Vec::new();
    let mut observations = Vec::new();
    for specification in plan.commands() {
        if specification.program() == "peritus-internal" {
            let outcome = match specification.arguments().first().map(String::as_str) {
                Some("source-readability") => GateOutcome::Required(source_layout::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.project().kind(),
                    specification.display(),
                    ownership,
                    cancellation,
                )),
                Some("artifact-csv-structure") => artifact_csv::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.display(),
                    request_context,
                    cancellation,
                ),
                Some("json-structure") => GateOutcome::Required(json_structure::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.display(),
                    cancellation,
                )),
                Some("sqlite-migration") => sqlite_migration::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.display(),
                    request_context,
                    cancellation,
                ),
                Some("yaml-structure") => GateOutcome::Required(yaml_structure::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.display(),
                    cancellation,
                )),
                _ => GateOutcome::Required(GateExecutionRecord {
                    command: specification.display(),
                    label: specification.label().to_owned(),
                    exit_code: None,
                    output: "unknown internal exact-target gate".to_owned(),
                }),
            };
            match outcome {
                GateOutcome::Required(record) => records.push(record),
                GateOutcome::Optional(observation) => observations.push(observation),
            }
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
    (records, observations)
}

fn execution_context(root: &Path, plan: &TargetGatePlan, request_context: &str) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-gate-execution-context-v1\0");
    hash_bytes(&mut hasher, std::env::consts::OS.as_bytes());
    hash_bytes(&mut hasher, std::env::consts::ARCH.as_bytes());
    hash_bytes(&mut hasher, root.as_os_str().as_encoded_bytes());
    hash_bytes(&mut hasher, request_context.as_bytes());
    let mut environment = std::env::vars_os().collect::<Vec<_>>();
    environment.sort_by(|left, right| {
        left.0
            .as_encoded_bytes()
            .cmp(right.0.as_encoded_bytes())
            .then_with(|| left.1.as_encoded_bytes().cmp(right.1.as_encoded_bytes()))
    });
    for (name, value) in environment {
        hasher.update([0]);
        hash_bytes(&mut hasher, name.as_encoded_bytes());
        hash_bytes(&mut hasher, value.as_encoded_bytes());
    }
    for command in plan.commands() {
        hasher.update([1]);
        hash_bytes(&mut hasher, command.display().as_bytes());
        hash_bytes(&mut hasher, command.current_dir().as_os_str().as_encoded_bytes());
        observe_program(&mut hasher, root, command.current_dir(), command.program());
    }
    Sha256Digest::new(hasher.finalize().into())
}

fn observe_program(hasher: &mut Sha256, root: &Path, current_dir: &Path, program: &str) {
    let resolved = if Path::new(program).components().count() > 1 {
        Some(root.join(current_dir).join(program))
    } else {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join(program))
                .find(|candidate| candidate.is_file())
        })
    };
    let Some(path) = resolved else {
        hasher.update([0]);
        hash_bytes(hasher, program.as_bytes());
        return;
    };
    hasher.update([1]);
    hash_bytes(hasher, path.as_os_str().as_encoded_bytes());
    match std::fs::read(&path) {
        Ok(bytes) => {
            hasher.update([1]);
            hash_bytes(hasher, &bytes);
        }
        Err(error) => {
            hasher.update([0]);
            hash_bytes(hasher, error.to_string().as_bytes());
        }
    }
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
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
    for observation in report.observations() {
        text.push_str(&format!(
            "\n[Optional observation: {}]\n$ {}\n{}\n",
            observation.label, observation.command, observation.output,
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
