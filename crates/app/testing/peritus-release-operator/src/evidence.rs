//! Candidate-bound inventory, SBOM, and provenance generation.

use std::{
    env,
    fs::OpenOptions,
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_release_artifacts::{
    ArtifactEntry, ArtifactInventory, ArtifactRole, BoundedId, BuildMaterial, MediaType,
    ProvenanceStatement, ProvenanceTimestamps, ReleaseBinding, ReleasePath, Sha256Digest,
    SpdxDocument, digest_bytes,
};

use crate::{
    cargo_graph, clock, error::OperatorError, files, package_record::PackageRecord, repository,
};

#[derive(Clone, Debug)]
pub struct EvidencePaths {
    pub archive: PathBuf,
    pub checksum: PathBuf,
    pub inventory: PathBuf,
    pub sbom: PathBuf,
    pub provenance: PathBuf,
}

pub fn generate(record_path: &Path) -> Result<(), OperatorError> {
    let root = repository::root()?;
    let record = PackageRecord::load(record_path)?;
    let binding = repository::binding(&root)?;
    let paths = paths(&root, &record)?;
    let inventory = inventory(&root, &record, binding.clone())?;
    files::write_atomic(&paths.inventory, &inventory.canonical_json()?)?;

    let sbom = SpdxDocument::new(
        binding.clone(),
        &BoundedId::new("peritus-release-operator-v1")?,
        clock::timestamp(record.finished())?,
        cargo_graph::components(&root)?,
    )?;
    files::write_atomic(&paths.sbom, &sbom.canonical_json()?)?;

    let lock_bytes = files::read_metadata(&root.join("Cargo.lock"))?;
    let provenance = ProvenanceStatement::new(
        binding.clone(),
        &inventory,
        builder_id()?,
        invocation_id(&binding)?,
        BoundedId::new("peritus/release-build/v1")?,
        ProvenanceTimestamps::new(
            clock::timestamp(record.started())?,
            clock::timestamp(record.finished())?,
        ),
        vec![
            BuildMaterial::new(
                format!(
                    "git+https://github.com/Corvidae-Coding-Projects/Project-Peritus@{}",
                    binding.candidate_commit().as_str()
                ),
                binding.source_tree_digest(),
            )?,
            BuildMaterial::new("file:Cargo.lock", digest_bytes(&lock_bytes))?,
        ],
    )?;
    files::write_atomic(&paths.provenance, &provenance.canonical_json()?)?;
    write_outputs(&paths)
}

pub fn paths(root: &Path, record: &PackageRecord) -> Result<EvidencePaths, OperatorError> {
    let archive = root.join(record.archive());
    let checksum = root.join(record.checksum());
    Ok(EvidencePaths {
        inventory: files::sibling(&archive, ".inventory.json")?,
        sbom: files::sibling(&archive, ".spdx.json")?,
        provenance: files::sibling(&archive, ".provenance.json")?,
        archive,
        checksum,
    })
}

fn inventory(
    root: &Path,
    record: &PackageRecord,
    binding: ReleaseBinding,
) -> Result<ArtifactInventory, OperatorError> {
    let archive_path = root.join(record.archive());
    let checksum_path = root.join(record.checksum());
    let (archive_length, archive_digest) = files::digest_file(&archive_path)?;
    let checksum_bytes = files::read_metadata(&checksum_path)?;
    let declared_digest = parse_checksum(&checksum_bytes)?;
    if declared_digest != archive_digest {
        return Err(OperatorError::metadata(
            "native package checksum does not match the exact archive bytes",
        ));
    }
    let archive_media = if archive_path.extension().and_then(|value| value.to_str()) == Some("zip")
    {
        "application/zip"
    } else {
        "application/gzip"
    };
    ArtifactInventory::new(
        binding,
        vec![
            ArtifactEntry::new(
                release_path(record.archive())?,
                archive_length,
                archive_digest,
                MediaType::new(archive_media)?,
                vec![ArtifactRole::Distribution],
            )?,
            ArtifactEntry::from_bytes(
                release_path(record.checksum())?,
                MediaType::new("text/plain")?,
                vec![ArtifactRole::Manifest],
                &checksum_bytes,
            )?,
        ],
    )
    .map_err(OperatorError::from)
}

fn parse_checksum(bytes: &[u8]) -> Result<Sha256Digest, OperatorError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| OperatorError::metadata("native package checksum is not UTF-8"))?;
    let digest = text.strip_suffix('\n').unwrap_or(text);
    if digest.contains(['\r', '\n']) {
        return Err(OperatorError::metadata(
            "native package checksum must contain one SHA-256 digest",
        ));
    }
    Sha256Digest::parse(digest).map_err(OperatorError::from)
}

fn release_path(path: &Path) -> Result<ReleasePath, OperatorError> {
    let text = path
        .to_str()
        .ok_or_else(|| OperatorError::metadata("release path is not UTF-8"))?
        .replace('\\', "/");
    ReleasePath::new(text).map_err(OperatorError::from)
}

fn builder_id() -> Result<BoundedId, OperatorError> {
    let os = repository::environment("RUNNER_OS")?;
    let arch = repository::environment("RUNNER_ARCH")?;
    BoundedId::new(format!("github-actions/{os}-{arch}")).map_err(OperatorError::from)
}

fn invocation_id(binding: &ReleaseBinding) -> Result<BoundedId, OperatorError> {
    let run = repository::environment("GITHUB_RUN_ID")?;
    let attempt = repository::environment("GITHUB_RUN_ATTEMPT")?;
    BoundedId::new(format!(
        "github/{run}/{attempt}/{}",
        binding.platform().as_str().replace('@', "-")
    ))
    .map_err(OperatorError::from)
}

fn write_outputs(paths: &EvidencePaths) -> Result<(), OperatorError> {
    let Some(output) = env::var_os("GITHUB_OUTPUT") else {
        return Ok(());
    };
    let mut file = OpenOptions::new().create(true).append(true).open(&output).map_err(|error| {
        OperatorError::io("open GitHub step output", PathBuf::from(&output), error)
    })?;
    for (name, path) in [
        ("archive", &paths.archive),
        ("checksum", &paths.checksum),
        ("inventory", &paths.inventory),
        ("sbom", &paths.sbom),
        ("provenance", &paths.provenance),
    ] {
        let value = path
            .to_str()
            .ok_or_else(|| OperatorError::metadata("GitHub output path is not UTF-8"))?;
        writeln!(file, "{name}={value}").map_err(|error| {
            OperatorError::io("write GitHub step output", PathBuf::from(&output), error)
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_checksum;

    #[test]
    fn checksum_parser_accepts_one_canonical_digest() {
        let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(parse_checksum(format!("{digest}\n").as_bytes()).is_ok());
        assert!(parse_checksum(format!("{digest}\n{digest}\n").as_bytes()).is_err());
        assert!(parse_checksum(digest.to_uppercase().as_bytes()).is_err());
    }
}
