//! Cross-file invariants for retained `HarnessBench` campaign evidence.

use std::collections::BTreeSet;

use super::model::{AgentEvidence, IdentityPolicy, ReportRequest, SelectedResult, TaskReport};
use crate::BenchmarkError;

pub(super) fn request(request: &ReportRequest) -> Result<(), BenchmarkError> {
    if request.expected_tasks == 0 {
        return Err(arguments("expected task count must be positive"));
    }
    let label = request.campaign_label.trim();
    if label.is_empty() || label.len() > 128 {
        return Err(arguments("campaign label must contain 1 through 128 bytes"));
    }
    if request.output.exists() {
        return Err(BenchmarkError::Workspace(format!(
            "report output already exists: {}",
            request.output.display()
        )));
    }
    Ok(())
}

pub(super) fn coverage(
    request: &ReportRequest,
    task_names: &BTreeSet<String>,
    selected: &[SelectedResult],
) -> Result<(), BenchmarkError> {
    if task_names.len() != request.expected_tasks {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench catalog contains {} tasks, expected {}",
            task_names.len(),
            request.expected_tasks
        )));
    }
    let result_names = selected.iter().map(|value| value.report.task_id.clone()).collect();
    if &result_names != task_names {
        let missing = task_names.difference(&result_names).cloned().collect::<Vec<_>>();
        let extra = result_names.difference(task_names).cloned().collect::<Vec<_>>();
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench result coverage differs from catalog; missing={missing:?}, extra={extra:?}"
        )));
    }
    Ok(())
}

pub(super) fn identities(
    request: &ReportRequest,
    selected: &[SelectedResult],
) -> Result<AgentEvidence, BenchmarkError> {
    let mut native_invocations = 0;
    let mut native_invocations_with_identity = 0;
    let mut source_revisions = BTreeSet::new();
    let mut binary_sha256s = BTreeSet::new();
    for result in selected {
        let Some(invocation) = &result.invocation else {
            require_identity(request, &result.report.task_id, "has no native invocation")?;
            continue;
        };
        native_invocations += 1;
        if invocation.task_id != result.report.task_id
            || invocation.session_id != result.report.session_id
        {
            return Err(BenchmarkError::Identity(format!(
                "HarnessBench invocation identity differs from selected result {}",
                result.report.task_id
            )));
        }
        let Some(identity) = &invocation.agent_identity else {
            require_identity(request, &result.report.task_id, "has no native build identity")?;
            continue;
        };
        validate_hex("source revision", &identity.source_revision, &[40, 64])?;
        validate_hex("binary SHA-256", &identity.binary_sha256, &[64])?;
        if identity.package_version.trim().is_empty() {
            return Err(BenchmarkError::Identity(format!(
                "HarnessBench task {} has an empty package version",
                result.report.task_id
            )));
        }
        native_invocations_with_identity += 1;
        source_revisions.insert(identity.source_revision.clone());
        binary_sha256s.insert(identity.binary_sha256.clone());
    }
    Ok(AgentEvidence {
        identity_policy: request.identity_policy,
        native_invocations,
        native_invocations_with_identity,
        source_revisions: source_revisions.into_iter().collect(),
        binary_sha256s: binary_sha256s.into_iter().collect(),
    })
}

pub(super) fn task(task: &TaskReport) -> Result<(), BenchmarkError> {
    if task.task_id.trim().is_empty()
        || task.session_id.trim().is_empty()
        || task.model_id.trim().is_empty()
        || task.candidate_results == 0
    {
        return Err(BenchmarkError::Workspace(
            "HarnessBench selected result has empty required identity".to_owned(),
        ));
    }
    validate_hex("result SHA-256", &task.result_sha256, &[64])?;
    crate::report_path::validate(&task.result_path, "HarnessBench result path")?;
    for (label, value) in [
        ("outcome", task.scores.outcome),
        ("process", task.scores.process),
        ("security", task.scores.security),
        ("combined", task.scores.combined),
    ] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(BenchmarkError::Workspace(format!(
                "HarnessBench task {} has invalid {label} score {value}",
                task.task_id
            )));
        }
    }
    if !task.elapsed_seconds.is_finite() || task.elapsed_seconds < 0.0 {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench task {} has invalid elapsed time",
            task.task_id
        )));
    }
    if !task.usage.available {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench task {} has no provider usage evidence",
            task.task_id
        )));
    }
    let derived = task
        .usage
        .input_tokens
        .checked_add(task.usage.output_tokens)
        .ok_or_else(|| BenchmarkError::Workspace("task token total overflowed".to_owned()))?;
    if derived != task.usage.total_tokens {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench task {} reports total tokens {}, expected {derived}",
            task.task_id, task.usage.total_tokens
        )));
    }
    Ok(())
}

fn require_identity(
    request: &ReportRequest,
    task_id: &str,
    detail: &str,
) -> Result<(), BenchmarkError> {
    if request.identity_policy == IdentityPolicy::RequireNative {
        return Err(BenchmarkError::Identity(format!("HarnessBench task {task_id} {detail}")));
    }
    Ok(())
}

fn validate_hex(label: &str, value: &str, lengths: &[usize]) -> Result<(), BenchmarkError> {
    if lengths.contains(&value.len())
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(BenchmarkError::Identity(format!(
        "HarnessBench {label} is not a full lowercase hexadecimal identity"
    )))
}

fn arguments(detail: impl Into<String>) -> BenchmarkError {
    BenchmarkError::Arguments(detail.into())
}
