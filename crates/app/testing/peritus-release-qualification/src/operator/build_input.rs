//! Artifact inventory and independent-build comparison assembly.

use std::path::Path;

use peritus_release_artifacts::{
    ArtifactEntry, ArtifactInventory, BoundedId, BuildWitness, MediaType, ReleaseBinding,
    ReleasePath, ReproducibilityComparison, compare_builds,
};

use super::{OperatorError, files, plan::BuildSpec};

pub(super) struct BuildInputs {
    pub inventory: ArtifactInventory,
    pub comparison: ReproducibilityComparison,
}

pub(super) fn assemble(
    binding: &ReleaseBinding,
    evidence_root: &Path,
    primary: &BuildSpec,
    independent: &BuildSpec,
) -> Result<BuildInputs, OperatorError> {
    let primary_inventory = inventory(binding, evidence_root, primary)?;
    let independent_inventory = inventory(binding, evidence_root, independent)?;
    let primary_witness =
        BuildWitness::from_inventory(BoundedId::new(&primary.builder_id)?, &primary_inventory)?;
    let independent_witness = BuildWitness::from_inventory(
        BoundedId::new(&independent.builder_id)?,
        &independent_inventory,
    )?;
    let comparison = compare_builds(&primary_witness, &independent_witness)?;
    Ok(BuildInputs { inventory: primary_inventory, comparison })
}

fn inventory(
    binding: &ReleaseBinding,
    evidence_root: &Path,
    build: &BuildSpec,
) -> Result<ArtifactInventory, OperatorError> {
    let mut entries = Vec::with_capacity(build.artifacts.len());
    for spec in &build.artifacts {
        let source = ReleasePath::new(&spec.source_path)?;
        let bytes = files::read_rooted(evidence_root, &source, "release artifact")?;
        entries.push(ArtifactEntry::from_bytes(
            ReleasePath::new(&spec.path)?,
            MediaType::new(&spec.media_type)?,
            spec.roles.clone(),
            &bytes,
        )?);
    }
    ArtifactInventory::new(binding.clone(), entries).map_err(OperatorError::from)
}
