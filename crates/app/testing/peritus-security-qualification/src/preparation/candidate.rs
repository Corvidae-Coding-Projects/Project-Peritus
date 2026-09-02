//! Deterministic integrated-candidate identity derived from committed source.

use std::fs;

use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::repository::CandidateRepository;
use crate::{IntegratedCandidate, candidate_json};

use super::PreparationError;

const ACCEPTANCE_PATHS: &[&str] = &[
    ".design/peritus-production-architecture.md",
    ".design/b2-acceptance-specification.md",
    "architecture.toml",
    "crates/foundation/peritus-quality-policy",
    "crates/foundation/peritus-spec",
];
const HARNESS_PATHS: &[&str] = &[
    "crates/orchestration/peritus-agent",
    "crates/orchestration/peritus-harness",
    "crates/orchestration/peritus-orchestrator",
];
const POLICY_PATHS: &[&str] = &[
    "architecture.toml",
    "crates/foundation/peritus-policy",
    "crates/foundation/peritus-quality-policy",
    "crates/foundation/peritus-security-policy",
    "security",
];
const PROVIDER_PATHS: &[&str] = &["crates/model"];
const RELEASE_PATHS: &[&str] = &[
    ".github/workflows/release.yml",
    "Cargo.lock",
    "Cargo.toml",
    "architecture.toml",
    "crates/app/peritus-cli",
    "crates/app/peritus-daemon",
    "crates/app/peritus-launcher",
    "crates/app/peritus-tui",
    "install.ps1",
    "install.sh",
    "release",
    "rust-toolchain.toml",
    "xtask/src/product_package",
    "xtask/src/product_package.rs",
    "xtask/src/release.rs",
];
const PLAN_PATHS: &[&str] = &[
    "crates/app/testing/peritus-security-qualification",
    "crates/foundation/peritus-security-policy",
    "docs/h0-security-qualification.md",
    "security",
];

#[derive(Deserialize)]
struct Manifest {
    workspace: Workspace,
}

#[derive(Deserialize)]
struct Workspace {
    package: WorkspacePackage,
}

#[derive(Deserialize)]
struct WorkspacePackage {
    repository: String,
}

pub(super) struct PreparedCandidate {
    pub(super) candidate: IntegratedCandidate,
    pub(super) bytes: Vec<u8>,
    pub(super) commit: String,
}

pub(super) fn prepare(
    repository: &CandidateRepository,
) -> Result<PreparedCandidate, PreparationError> {
    let source = repository.source_digest()?;
    let acceptance = component_id(
        b"peritus.h0.acceptance-spec.v1",
        repository.archive_digest(ACCEPTANCE_PATHS)?,
    );
    let harness = component_id(b"peritus.h0.harness.v1", repository.archive_digest(HARNESS_PATHS)?);
    let policy = component_id(b"peritus.h0.policy.v1", repository.archive_digest(POLICY_PATHS)?);
    let provider =
        component_id(b"peritus.h0.provider-profile.v1", repository.archive_digest(PROVIDER_PATHS)?);
    let workspace = component_id(
        b"peritus.h0.workspace-lineage.v1",
        crate::digest_bytes(repository_url(repository)?.as_bytes()),
    );
    let revision_value = nonzero_revision(source);
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new(acceptance)
            .map_err(|_| PreparationError::Metadata("derived acceptance identity is zero"))?,
        HarnessId::new(harness)
            .map_err(|_| PreparationError::Metadata("derived harness identity is zero"))?,
        WorkspaceId::new(workspace)
            .map_err(|_| PreparationError::Metadata("derived workspace identity is zero"))?,
        Generation::first(),
        RevisionNumber::new(revision_value)
            .map_err(|_| PreparationError::Metadata("derived workspace revision is zero"))?,
        PolicyId::new(policy)
            .map_err(|_| PreparationError::Metadata("derived policy identity is zero"))?,
        ProviderProfileId::new(provider)
            .map_err(|_| PreparationError::Metadata("derived provider identity is zero"))?,
    );
    let candidate = IntegratedCandidate::new(
        revision,
        source,
        repository.archive_digest(RELEASE_PATHS)?,
        repository.archive_digest(PLAN_PATHS)?,
    );
    Ok(PreparedCandidate {
        candidate,
        bytes: candidate_json(candidate)?,
        commit: repository.head_commit()?,
    })
}

fn repository_url(repository: &CandidateRepository) -> Result<String, PreparationError> {
    let path = repository.root().join("Cargo.toml");
    let text = fs::read_to_string(&path)
        .map_err(|source| PreparationError::io("read workspace manifest", &path, source))?;
    let manifest: Manifest = toml::from_str(&text)?;
    let value = manifest.workspace.package.repository.trim();
    if value.is_empty() {
        return Err(PreparationError::Metadata("workspace repository identity is empty"));
    }
    Ok(value.to_owned())
}

fn component_id(domain: &[u8], digest: Sha256Digest) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update([0]);
    hasher.update(digest.into_bytes());
    let complete: [u8; 32] = hasher.finalize().into();
    let mut identifier = [0_u8; 16];
    identifier.copy_from_slice(&complete[..16]);
    if identifier.iter().all(|byte| *byte == 0) {
        identifier[15] = 1;
    }
    identifier
}

fn nonzero_revision(source: Sha256Digest) -> u64 {
    let bytes = source.into_bytes();
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&bytes[..8]);
    u64::from_be_bytes(prefix).max(1)
}
