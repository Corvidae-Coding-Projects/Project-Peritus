//! Retention and publication of externally signed release evidence.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    error::OperatorError,
    evidence::{self, EvidencePaths},
    files,
    package_record::PackageRecord,
    repository,
};

const REPOSITORY: &str = "Corvidae-Coding-Projects/Project-Peritus";
const RELEASE_WORKFLOW: &str =
    "Corvidae-Coding-Projects/Project-Peritus/.github/workflows/release.yml";
const SLSA_PROVENANCE: &str = "https://slsa.dev/provenance/v1";
const SPDX_DOCUMENT: &str = "https://spdx.dev/Document/v2.3";

pub fn retain_and_upload(
    record_path: &Path,
    provenance_bundle: &Path,
    sbom_bundle: &Path,
) -> Result<(), OperatorError> {
    let root = repository::root()?;
    let record = PackageRecord::load(record_path)?;
    let paths = evidence::paths(&root, &record)?;
    require_generated(&paths)?;
    verify_attestation(&root, &paths.archive, provenance_bundle, SLSA_PROVENANCE)?;
    verify_attestation(&root, &paths.archive, sbom_bundle, SPDX_DOCUMENT)?;
    let retained_provenance = files::sibling(&paths.archive, ".provenance.sigstore.jsonl")?;
    let retained_sbom = files::sibling(&paths.archive, ".sbom.sigstore.jsonl")?;
    retain_bundle(provenance_bundle, &retained_provenance)?;
    retain_bundle(sbom_bundle, &retained_sbom)?;
    let evidence_checksums = files::sibling(&paths.archive, ".evidence.sha256")?;
    let mut assets = vec![
        paths.archive,
        paths.checksum,
        paths.inventory,
        paths.sbom,
        paths.provenance,
        retained_provenance,
        retained_sbom,
    ];
    write_checksums(&evidence_checksums, &assets)?;
    assets.push(evidence_checksums);
    upload(&root, &assets)
}

fn verify_attestation(
    root: &Path,
    archive: &Path,
    bundle: &Path,
    predicate_type: &'static str,
) -> Result<(), OperatorError> {
    let tag = repository::environment("GITHUB_REF_NAME")?;
    let source_digest = repository::environment("GITHUB_SHA")?;
    let source_ref = format!("refs/tags/{tag}");
    let status = Command::new("gh")
        .current_dir(root)
        .args(["attestation", "verify"])
        .arg(archive)
        .args(["--bundle"])
        .arg(bundle)
        .args([
            "--repo",
            REPOSITORY,
            "--signer-workflow",
            RELEASE_WORKFLOW,
            "--source-digest",
            &source_digest,
            "--source-ref",
            &source_ref,
            "--predicate-type",
            predicate_type,
            "--deny-self-hosted-runners",
        ])
        .status()
        .map_err(|error| {
            OperatorError::io("start release attestation verification", "gh", error)
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(OperatorError::Command { operation: "verify signed release attestation", status })
    }
}

fn require_generated(paths: &EvidencePaths) -> Result<(), OperatorError> {
    for path in [&paths.archive, &paths.checksum, &paths.inventory, &paths.sbom, &paths.provenance]
    {
        if !path.is_file() {
            return Err(OperatorError::metadata(format!(
                "required release evidence {} is missing",
                path.display()
            )));
        }
    }
    Ok(())
}

fn retain_bundle(source: &Path, destination: &Path) -> Result<(), OperatorError> {
    let bytes = files::read_metadata(source)?;
    if bytes.is_empty() {
        return Err(OperatorError::metadata("GitHub attestation bundle is empty"));
    }
    files::write_atomic(destination, &bytes)
}

fn write_checksums(path: &Path, assets: &[PathBuf]) -> Result<(), OperatorError> {
    let mut lines = String::new();
    for asset in assets {
        let (_, digest) = files::digest_file(asset)?;
        let name = asset
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| OperatorError::metadata("release asset name is not UTF-8"))?;
        writeln!(lines, "{}  {name}", digest.to_hex())
            .map_err(|_| OperatorError::metadata("format evidence checksum line"))?;
    }
    files::write_atomic(path, lines.as_bytes())
}

fn upload(root: &Path, assets: &[PathBuf]) -> Result<(), OperatorError> {
    let tag = repository::environment("GITHUB_REF_NAME")?;
    let mut command = Command::new("gh");
    command.current_dir(root).args(["release", "upload", &tag]);
    command.args(assets).arg("--clobber");
    let status = command
        .status()
        .map_err(|error| OperatorError::io("start GitHub release upload", "gh", error))?;
    if status.success() {
        Ok(())
    } else {
        Err(OperatorError::Command { operation: "upload signed release evidence", status })
    }
}
