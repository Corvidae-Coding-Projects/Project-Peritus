//! Versioned local image manifests: exact programs, supplementary resources, and finite policy.

use std::path::{Path, PathBuf};

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_eval::{ExecutionBinding, FrozenEvaluationProfile};
use peritus_scheduler::{ResourceKind, ResourceVector};
use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};

use super::super::execution::{
    ExecutableInput, ExecutionArgument, FrozenSupportFile, LocalProcessLimits, LocalProcessSandbox,
};

/// Frozen immutable supplementary program input.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSupportFile {
    name: String,
    artifact: [u8; 32],
}
impl LocalSupportFile {
    /// Declares a flat immutable resource name and its exact C0 artifact digest.
    pub fn new(name: String, artifact: ArtifactDigest) -> Result<Self, String> {
        if name.is_empty()
            || name.len() > 64
            || name == "."
            || name == ".."
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        {
            return Err("local program supplementary name must be a bounded flat identifier".into());
        }
        Ok(Self { name, artifact: *artifact.as_bytes() })
    }
}

/// Exact artifact-backed executable, arguments, and supplementary resources.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProgram {
    artifact: [u8; 32],
    size: u64,
    arguments: Vec<String>,
    support_files: Vec<LocalSupportFile>,
}
impl LocalProgram {
    /// Freezes a real executable artifact descriptor. Only the native gateway may run it.
    pub fn new(
        artifact: ArtifactDigest,
        size: u64,
        arguments: Vec<String>,
        mut support_files: Vec<LocalSupportFile>,
    ) -> Result<Self, String> {
        support_files.sort_by(|a, b| a.name.cmp(&b.name));
        let value = Self { artifact: *artifact.as_bytes(), size, arguments, support_files };
        value.validate()?;
        Ok(value)
    }
    /// Exact executable artifact, which must also be declared by the corresponding E1 arm.
    #[must_use]
    pub const fn artifact(&self) -> ArtifactDigest {
        ArtifactDigest::new(self.artifact)
    }
    /// Declared immutable executable byte count.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }
    /// Fingerprint of the complete verifier implementation, resources, and argument contract.
    pub fn verifier_digest(&self) -> Result<Sha256Digest, String> {
        Ok(peritus_codec::sha256(&serde_json::to_vec(self).map_err(detail)?))
    }
    fn validate(&self) -> Result<(), String> {
        if self.size == 0
            || self.size > 256 * 1024 * 1024
            || self.arguments.len() > 256
            || self.arguments.iter().any(|a| a.len() > 16 * 1024 || a.contains('\0'))
            || self.arguments.iter().map(String::len).sum::<usize>() > 64 * 1024
            || self.support_files.len() > 16
            || self.support_files.windows(2).any(|p| p[0].name >= p[1].name)
        {
            return Err("local program descriptor exceeds its bounded canonical contract".into());
        }
        for file in &self.support_files {
            LocalSupportFile::new(file.name.clone(), ArtifactDigest::new(file.artifact))?;
        }
        Ok(())
    }
    pub(super) fn verify(&self, store: &ArtifactStore) -> Result<(), String> {
        if store.verify(self.artifact()).map_err(detail)?.size() != self.size {
            return Err("local program executable differs from its frozen declared size".into());
        }
        for file in &self.support_files {
            if store.verify(ArtifactDigest::new(file.artifact)).map_err(detail)?.size()
                > 16 * 1024 * 1024
            {
                return Err("local program supplementary artifact exceeds its bound".into());
            }
        }
        Ok(())
    }
    pub(super) fn input(&self) -> ExecutableInput {
        ExecutableInput {
            digest: self.artifact(),
            size: self.size,
            arguments: self.arguments.iter().cloned().map(ExecutionArgument::Descriptor).collect(),
            support_files: self
                .support_files
                .iter()
                .map(|f| FrozenSupportFile {
                    name: f.name.clone(),
                    digest: ArtifactDigest::new(f.artifact),
                })
                .collect(),
        }
    }
}

/// Complete finite per-stage native process ceilings.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProcessLimitsDecl {
    wall_millis: u64,
    cpu_millis: u64,
    memory_bytes: u64,
    disk_bytes: u64,
    output_bytes: u64,
    process_count: u64,
}
impl LocalProcessLimitsDecl {
    /// Validates finite wall, CPU, memory, disk, output, and process limits.
    pub fn new(
        wall_millis: u64,
        cpu_millis: u64,
        memory_bytes: u64,
        disk_bytes: u64,
        output_bytes: u64,
        process_count: u64,
    ) -> Result<Self, String> {
        let value =
            Self { wall_millis, cpu_millis, memory_bytes, disk_bytes, output_bytes, process_count };
        value.native().resources()?;
        Ok(value)
    }
    pub(super) const fn native(self) -> LocalProcessLimits {
        LocalProcessLimits {
            wall_millis: self.wall_millis,
            cpu_millis: self.cpu_millis,
            memory_bytes: self.memory_bytes,
            disk_bytes: self.disk_bytes,
            output_bytes: self.output_bytes,
            process_count: self.process_count,
        }
    }
}

/// Trusted installed Linux components; no value here grants execution or network authority.
#[derive(Clone, Debug)]
pub struct LocalProcessBackend {
    native: LocalProcessSandbox,
}
impl LocalProcessBackend {
    /// Declares exact absolute installed bubblewrap/helper/delegated cgroup paths.
    pub fn new(bubblewrap: PathBuf, helper: PathBuf, cgroup_root: PathBuf) -> Result<Self, String> {
        if !bubblewrap.is_absolute() || !helper.is_absolute() || !cgroup_root.is_absolute() {
            return Err("native local-process component paths must be absolute".into());
        }
        Ok(Self { native: LocalProcessSandbox { bubblewrap, helper, cgroup_root } })
    }
    pub(super) const fn native(&self) -> &LocalProcessSandbox {
        &self.native
    }
    #[cfg(target_os = "linux")]
    pub(super) fn descriptor_digest(&self, scratch: &Path) -> Result<Sha256Digest, String> {
        use peritus_process::NativeSandboxBackend as _;
        let config = peritus_sandbox_linux::LinuxBackendConfig::new(
            scratch.to_path_buf(),
            vec![],
            self.native.bubblewrap.clone(),
            self.native.helper.clone(),
            self.native.cgroup_root.clone(),
            None,
        )
        .map_err(detail)?;
        let backend = peritus_sandbox_linux::LinuxBackend::new(config.with_private_filesystem())
            .map_err(detail)?;
        Ok(backend.descriptor().digest())
    }
    #[cfg(not(target_os = "linux"))]
    pub(super) fn descriptor_digest(&self, _scratch: &Path) -> Result<Sha256Digest, String> {
        Err("local-process RSI requires the Linux private-filesystem native backend".into())
    }
}

/// Canonical image manifest for an explicit `LocalProcess` evaluation campaign.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProcessCatalog {
    version: u16,
    baseline: LocalProgram,
    candidate: LocalProgram,
    evaluator: LocalProgram,
    limits: LocalProcessLimitsDecl,
}
impl LocalProcessCatalog {
    /// Complete direct program dependencies for immutable C0 retention before process effects.
    pub(crate) fn artifact_roots(&self) -> Vec<ArtifactDigest> {
        let mut roots = Vec::new();
        for program in [self.baseline(), self.candidate(), self.evaluator()] {
            roots.push(program.artifact());
            roots.extend(
                program.support_files.iter().map(|file| ArtifactDigest::new(file.artifact)),
            );
        }
        roots.sort();
        roots.dedup();
        roots
    }
    /// Freezes the exact baseline, candidate, independent evaluator, and native limits.
    pub fn new(
        baseline: LocalProgram,
        candidate: LocalProgram,
        evaluator: LocalProgram,
        limits: LocalProcessLimitsDecl,
    ) -> Result<Self, String> {
        let value = Self { version: 1, baseline, candidate, evaluator, limits };
        value.validate()?;
        Ok(value)
    }
    /// Baseline executable descriptor.
    #[must_use]
    pub const fn baseline(&self) -> &LocalProgram {
        &self.baseline
    }
    /// Candidate executable descriptor.
    #[must_use]
    pub const fn candidate(&self) -> &LocalProgram {
        &self.candidate
    }
    /// Independent verifier descriptor.
    #[must_use]
    pub const fn evaluator(&self) -> &LocalProgram {
        &self.evaluator
    }
    /// Complete stable image-manifest bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(detail)?;
        if bytes.len() > 1024 * 1024 {
            return Err("local image manifest exceeds 1 MiB".into());
        }
        Ok(bytes)
    }
    /// Restores only exact canonical versioned bytes, without admitting any execution.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 1024 * 1024 {
            return Err("local image manifest exceeds 1 MiB".into());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(detail)?;
        if value.canonical_bytes()? != bytes {
            return Err("local image manifest is not canonical".into());
        }
        Ok(value)
    }
    /// Immutable image digest bound by the frozen E3 profile.
    pub fn digest(&self) -> Result<Sha256Digest, String> {
        Ok(peritus_codec::sha256(&self.canonical_bytes()?))
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported local image manifest version".into());
        }
        self.baseline.validate()?;
        self.candidate.validate()?;
        self.evaluator.validate()?;
        self.limits.native().resources()?;
        Ok(())
    }
    pub(super) const fn limits(&self) -> LocalProcessLimits {
        self.limits.native()
    }
    /// Digest of the fixed private-filesystem, denied-network/secret, finite resource template.
    pub fn sandbox_policy_digest(&self) -> Result<Sha256Digest, String> {
        let mut bytes=b"peritus.rsi.local-private-fs-policy.v1\0cleared-env\0deny-network\0deny-secrets\0runtime-lib-only\0pipes-closed-stdin\0tree-required\0fd256\0events8192\0".to_vec();
        bytes.extend(serde_json::to_vec(&self.limits).map_err(detail)?);
        Ok(peritus_codec::sha256(&bytes))
    }
    /// Exact cleared environment template. Actual plan variables must remain empty at launch.
    #[must_use]
    pub fn environment_digest(&self) -> Sha256Digest {
        peritus_codec::sha256(b"peritus.rsi.local-environment.v1\0cleared\0no-literals\0")
    }
    /// Rejects hidden task artifacts grafted into either candidate-visible frozen image.
    ///
    /// Supplemental image resources are immutable program dependencies, never a route around
    /// the candidate/evaluator task boundary. This checks the complete corpus before effects.
    ///
    /// # Errors
    /// Rejects candidate-visible executable or support artifacts shared with hidden task inputs.
    pub fn validate_task_isolation(&self, profile: &FrozenEvaluationProfile) -> Result<(), String> {
        let hidden: std::collections::BTreeSet<_> = profile
            .dataset()
            .tasks()
            .iter()
            .map(|task| task.evaluator_input().artifact())
            .collect();
        for program in [self.baseline(), self.candidate()] {
            if hidden.contains(&program.artifact())
                || program
                    .support_files
                    .iter()
                    .any(|file| hidden.contains(&ArtifactDigest::new(file.artifact)))
            {
                return Err("local candidate image aliases a sealed evaluator task artifact".into());
            }
        }
        Ok(())
    }
    /// Builds a frozen `LocalProcess` binding from a real native backend probe, not caller facts.
    /// Dynamic per-child sandbox/preparation digests are retained in actual process receipts;
    /// frozen fields identify this exact policy template and probed native descriptor.
    pub fn execution_binding(
        &self,
        backend: &LocalProcessBackend,
        scratch: &Path,
        resources: ResourceVector,
        deadline_micros: u64,
        concurrency: u16,
    ) -> Result<ExecutionBinding, String> {
        self.validate_resources(&resources, deadline_micros)?;
        ExecutionBinding::new(
            self.sandbox_policy_digest()?,
            backend.descriptor_digest(scratch)?,
            self.environment_digest(),
            self.digest()?,
            resources,
            deadline_micros,
            concurrency,
            true,
            true,
        )
        .map_err(detail)?
        .with_local_process()
        .map_err(detail)
    }
    pub(super) fn validate_resources(
        &self,
        resources: &ResourceVector,
        deadline: u64,
    ) -> Result<(), String> {
        let quantity = |kind| {
            resources.entries().iter().find(|e| e.kind() == kind).map(|e| e.quantity().get())
        };
        if self.limits.wall_millis.checked_mul(1000).is_none_or(|w| w > deadline)
            || quantity(ResourceKind::CPU).is_none_or(|v| v < 1)
            || quantity(ResourceKind::MEMORY_BYTES).is_none_or(|v| v < self.limits.memory_bytes)
            || quantity(ResourceKind::PROCESS).is_none_or(|v| v < self.limits.process_count)
            || quantity(ResourceKind::NETWORK).is_some()
            || quantity(ResourceKind::GPU).is_some()
        {
            return Err("local native ceilings exceed frozen D3 resources/deadline or request forbidden network/GPU slots".into());
        }
        Ok(())
    }
}
fn detail(error: impl std::fmt::Display) -> String {
    format!("local RSI catalog: {error}")
}
