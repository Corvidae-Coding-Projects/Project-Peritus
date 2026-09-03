//! External adapter handshake and pre-admission path preparation.

use std::{fs, path::PathBuf, time::Duration};

use peritus_product_runner::ProductDeliveryScope;
use sha2::{Digest, Sha256};

use crate::{
    BenchmarkError,
    agent::run_id,
    args::{HarnessBenchInput, TerminalBenchInput},
    evidence::{BenchmarkSuite, HandshakeReport, ResourceReport, TraceUsage},
    identity::{BenchmarkAgentIdentity, INVOCATION_REPORT_SCHEMA_VERSION},
    publication::AtomicPublisher,
    session::BenchmarkSession,
    settlement::{InvocationGuard, ReportSeed, workspace_id},
    trace,
};

const PRODUCT_PROTOCOL_VERSION: u32 = 1;

/// Fully prepared invocation. Constructing this value is the admission boundary.
pub struct AdmittedInvocation {
    pub guard: InvocationGuard,
    pub prompt: String,
    pub conversation: BenchmarkSession,
    pub evidence_dir: PathBuf,
    pub sandbox: Option<PathBuf>,
    pub max_elapsed: Duration,
    pub delivery_scope: ProductDeliveryScope,
}

struct AdmissionSpec {
    suite: BenchmarkSuite,
    workspace: PathBuf,
    evidence_dir: PathBuf,
    recovery_dir: PathBuf,
    sandbox: Option<PathBuf>,
    prompt_file: PathBuf,
    session_id: String,
    task_id: String,
    model_id: String,
    adapter_schema_version: u32,
    suite_revision: String,
    max_elapsed: Duration,
    delivery_scope: ProductDeliveryScope,
    usage_proxy: Option<PathBuf>,
}

pub fn harnessbench(input: HarnessBenchInput) -> Result<AdmittedInvocation, BenchmarkError> {
    let max_elapsed = crate::deadline::harnessbench_horizon(&input.task_id)?;
    let sandbox = canonical_directory(&input.sandbox, "sandbox")?;
    prepare_admission(AdmissionSpec {
        suite: BenchmarkSuite::HarnessBench,
        workspace: input.workspace,
        evidence_dir: sandbox.join("peritus-benchmark"),
        recovery_dir: sandbox.join("peritus-recovery"),
        sandbox: Some(sandbox.clone()),
        prompt_file: input.prompt_file,
        session_id: input.session_id,
        task_id: input.task_id,
        model_id: input.model_id,
        adapter_schema_version: input.adapter_schema_version,
        suite_revision: input.suite_revision,
        max_elapsed,
        delivery_scope: ProductDeliveryScope::WorkspaceChanges,
        usage_proxy: Some(sandbox.join("usage-proxy")),
    })
}

pub fn terminalbench(input: TerminalBenchInput) -> Result<AdmittedInvocation, BenchmarkError> {
    let recovery_dir = input.evidence_dir.parent().map_or_else(
        || PathBuf::from("peritus-recovery"),
        |parent| parent.join("peritus-recovery"),
    );
    prepare_admission(AdmissionSpec {
        suite: BenchmarkSuite::TerminalBench,
        workspace: input.workspace,
        evidence_dir: input.evidence_dir,
        recovery_dir,
        sandbox: None,
        prompt_file: input.prompt_file,
        session_id: input.session_id,
        task_id: input.task_id,
        model_id: input.model_id,
        adapter_schema_version: input.adapter_schema_version,
        suite_revision: input.suite_revision,
        max_elapsed: input.max_elapsed,
        delivery_scope: ProductDeliveryScope::AuthorizedExternalEffects,
        usage_proxy: None,
    })
}

fn prepare_admission(spec: AdmissionSpec) -> Result<AdmittedInvocation, BenchmarkError> {
    validate_schema(spec.adapter_schema_version)?;
    validate_revision(&spec.suite_revision)?;
    let workspace = canonical_directory(&spec.workspace, "workspace")?;
    let evidence_dir = prepare_and_canonicalize(&spec.evidence_dir, "evidence directory")?;
    let recovery_dir = prepare_and_canonicalize(&spec.recovery_dir, "recovery directory")?;
    let identity = BenchmarkAgentIdentity::current()?;
    let prompt = fs::read_to_string(&spec.prompt_file).map_err(|error| {
        BenchmarkError::filesystem("read benchmark prompt", &spec.prompt_file, error)
    })?;
    let conversation = BenchmarkSession::open(
        &evidence_dir,
        &spec.session_id,
        &spec.task_id,
        &spec.prompt_file,
        prompt.clone(),
    )?;
    let trace_path = conversation.current_trace_path();
    trace::prepare(&trace_path)?;
    let declared_provider_routes = crate::providers::declared_routes()?;
    let config_digest = config_digest(&spec, &workspace);
    let invocation_name =
        format!("{}-turn-{:04}", &config_digest[..16], conversation.turn_number());
    let publisher = AtomicPublisher::prepare(&evidence_dir, &recovery_dir, invocation_name)?;
    let run_id = run_id(&spec.session_id, &spec.task_id)?;
    let seed = ReportSeed {
        suite: spec.suite,
        handshake: HandshakeReport {
            adapter_schema_version: spec.adapter_schema_version,
            product_protocol_version: PRODUCT_PROTOCOL_VERSION,
            suite_revision: spec.suite_revision,
            config_digest,
            workspace_available: true,
            workspace: workspace.clone(),
            trace_path: trace_path.clone(),
            evidence_path: evidence_dir.clone(),
            recovery_path: recovery_dir,
            agent_identity: identity.clone(),
            provider_routes: declared_provider_routes,
        },
        agent_identity: identity,
        task_id: spec.task_id,
        session_id: spec.session_id,
        harness_model_id: spec.model_id,
        workspace: workspace.clone(),
        trace_path,
        conversation_turn: conversation.turn_number(),
        writer: format!("openai/{}", crate::providers::WRITER_MODEL),
        reviewer: format!("anthropic/{}", crate::providers::REVIEWER_MODEL),
        run_id,
        workspace_id: workspace_id(&workspace)?,
        baseline: None,
        provider_routes: Vec::new(),
        session_trace_paths: Vec::new(),
        usage_proxy: spec.usage_proxy,
        projected_responses: 0,
        usage: TraceUsage::default(),
        resources: ResourceReport::default(),
        last_observation_path: None,
        relocatable_paths: None,
    };
    Ok(AdmittedInvocation {
        guard: InvocationGuard::new(seed, publisher),
        prompt,
        conversation,
        evidence_dir,
        sandbox: spec.sandbox,
        max_elapsed: spec.max_elapsed,
        delivery_scope: spec.delivery_scope,
    })
}

const fn validate_schema(actual: u32) -> Result<(), BenchmarkError> {
    if actual == INVOCATION_REPORT_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(BenchmarkError::UnsupportedSchema {
            actual,
            expected: INVOCATION_REPORT_SCHEMA_VERSION,
        })
    }
}

fn validate_revision(value: &str) -> Result<(), BenchmarkError> {
    if matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(BenchmarkError::Arguments(
            "suite revision must be a full lowercase Git object ID".to_owned(),
        ))
    }
}

fn canonical_directory(path: &std::path::Path, label: &str) -> Result<PathBuf, BenchmarkError> {
    if !path.exists() {
        return Err(BenchmarkError::MissingWorkspace { path: path.to_path_buf() });
    }
    let canonical = path.canonicalize().map_err(|error| {
        BenchmarkError::filesystem("canonicalize benchmark directory", path, error)
    })?;
    if !canonical.is_dir() {
        return Err(BenchmarkError::Workspace(format!(
            "{label} is not a directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn prepare_and_canonicalize(
    path: &std::path::Path,
    label: &str,
) -> Result<PathBuf, BenchmarkError> {
    fs::create_dir_all(path).map_err(|error| {
        BenchmarkError::filesystem("create benchmark evidence path", path, error)
    })?;
    canonical_directory(path, label)
}

fn config_digest(spec: &AdmissionSpec, workspace: &std::path::Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.external-admission.v1\0");
    for field in [
        format!("{:?}", spec.suite),
        workspace.to_string_lossy().into_owned(),
        spec.task_id.clone(),
        spec.session_id.clone(),
        spec.model_id.clone(),
        spec.suite_revision.clone(),
        spec.max_elapsed.as_secs().to_string(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update(b"\0");
    }
    lowercase_hex(&hasher.finalize())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_stale_schema_and_missing_workspace_before_admission() {
        assert!(matches!(
            validate_schema(INVOCATION_REPORT_SCHEMA_VERSION - 1),
            Err(BenchmarkError::UnsupportedSchema { .. })
        ));
        let missing = PathBuf::from("/definitely/missing/peritus-workspace");
        assert!(matches!(
            canonical_directory(&missing, "workspace"),
            Err(BenchmarkError::MissingWorkspace { .. })
        ));
    }

    #[test]
    fn accepts_only_full_lowercase_suite_revisions() {
        validate_revision("0123456789abcdef0123456789abcdef01234567").expect("revision");
        assert!(validate_revision("0123456").is_err());
        assert!(validate_revision("ABCDEF0123456789ABCDEF0123456789ABCDEF01").is_err());
    }
}
