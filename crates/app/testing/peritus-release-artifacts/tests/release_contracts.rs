//! Release artifact, SBOM, provenance, signature, and reproducibility contracts.

use peritus_release_artifacts::{
    ArtifactEntry, ArtifactInventory, ArtifactRole, BoundedId, BuildMaterial, BuildWitness,
    CandidateCommit, MediaType, PlatformTriple, ProvenanceStatement, ProvenanceTimestamps,
    ReleaseBinding, ReleasePath, ReleaseVersion, SpdxComponent, SpdxDocument, SpdxTimestamp,
    ToolchainId, compare_builds, digest_bytes,
};

fn binding() -> ReleaseBinding {
    ReleaseBinding::new(
        CandidateCommit::new("2".repeat(40)).expect("full commit"),
        ReleaseVersion::new("1.0.0-rc.1").expect("version"),
        ToolchainId::new("rust-1.97.1-verus-0.2026.08.09").expect("toolchain"),
        PlatformTriple::new("x86_64-unknown-linux-gnu@linux-6.6").expect("platform"),
        digest_bytes(b"source tree"),
    )
}

fn inventory(bytes: &[u8]) -> ArtifactInventory {
    ArtifactInventory::new(
        binding(),
        vec![
            ArtifactEntry::from_bytes(
                ReleasePath::new("dist/peritus-linux-x86_64.tar.zst").expect("path"),
                MediaType::new("application/zstd").expect("media type"),
                vec![ArtifactRole::Distribution],
                bytes,
            )
            .expect("artifact"),
            ArtifactEntry::from_bytes(
                ReleasePath::new("bin/peritus").expect("path"),
                MediaType::new("application/octet-stream").expect("media type"),
                vec![ArtifactRole::Executable],
                b"executable",
            )
            .expect("artifact"),
        ],
    )
    .expect("inventory")
}

#[test]
fn independent_builds_compare_exact_paths_sizes_and_hashes() {
    let first = BuildWitness::from_inventory(
        BoundedId::new("builder-a").expect("builder"),
        &inventory(b"package-a"),
    )
    .expect("witness");
    let second = BuildWitness::from_inventory(
        BoundedId::new("builder-b").expect("builder"),
        &inventory(b"package-b"),
    )
    .expect("witness");
    let comparison = compare_builds(&first, &second).expect("comparison");
    assert!(!comparison.is_reproducible());
    assert_eq!(comparison.differences().len(), 1);
}

#[test]
fn same_builder_cannot_claim_independent_reproducibility() {
    let first = BuildWitness::from_inventory(
        BoundedId::new("builder-a").expect("builder"),
        &inventory(b"package"),
    )
    .expect("witness");
    let second = first.clone();
    assert!(compare_builds(&first, &second).is_err());
}

#[test]
fn spdx_and_provenance_are_deterministic_and_candidate_bound() {
    let release = inventory(b"package");
    let component_id = BoundedId::new("peritus-cli").expect("component ID");
    let component = SpdxComponent::new(
        &component_id,
        "peritus-cli",
        "1.0.0-rc.1",
        "Organization: Corvidae Coding Projects",
        "NOASSERTION",
        "MIT",
        digest_bytes(b"component source"),
    )
    .expect("component");
    let creator = BoundedId::new("peritus-h4-sbom-generator-v1").expect("creator");
    let timestamp = SpdxTimestamp::new("2026-08-27T12:00:00Z").expect("timestamp");
    let sbom =
        SpdxDocument::new(binding(), &creator, timestamp.clone(), vec![component]).expect("SBOM");
    assert_eq!(sbom.digest().expect("digest"), sbom.digest().expect("digest"));

    let provenance = ProvenanceStatement::new(
        binding(),
        &release,
        BoundedId::new("builder-a").expect("builder"),
        BoundedId::new("invocation-123").expect("invocation"),
        BoundedId::new("peritus/release-build/v1").expect("build type"),
        ProvenanceTimestamps::new(timestamp.clone(), timestamp),
        vec![
            BuildMaterial::new(
                "git+https://github.com/Corvidae-Coding-Projects/Project-Peritus",
                digest_bytes(b"source tree"),
            )
            .expect("material"),
        ],
    )
    .expect("provenance");
    assert_eq!(provenance.digest().expect("digest"), provenance.digest().expect("digest"));
}
