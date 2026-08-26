//! Canonical promotion, rollback, and publication actions.

use crate::{
    ActivationId, CampaignPublication, EvolutionError, PromotionProposal, RollbackProposal,
    VariantId,
};
use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_types::EvidenceId;

use super::{super::scalar, binding};

pub(super) fn write_promotion(
    writer: &mut CanonicalWriter,
    value: &PromotionProposal,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.project_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.campaign_id().as_bytes()).map_err(scalar::codec)?;
    binding::write_production(writer, value.current())?;
    binding::write_production(writer, value.candidate())?;
    writer.write_fixed(value.variant_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.variant_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.attribution_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evaluation_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_option_tag(value.review().is_some()).map_err(scalar::codec)?;
    if let Some(review) = value.review() {
        binding::write_review(writer, review)?;
    }
    writer.write_fixed(value.policy_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.selection_digest().as_bytes()).map_err(scalar::codec)?;
    binding::write_production(writer, value.rollback_target())?;
    writer.write_fixed(value.evidence_bundle_artifact().as_bytes()).map_err(scalar::codec)
}

pub(super) fn promotion(
    reader: &mut CanonicalReader<'_>,
) -> Result<PromotionProposal, EvolutionError> {
    let project = scalar::project_id(reader).map_err(scalar::codec)?;
    let campaign = scalar::campaign_id(reader).map_err(scalar::codec)?;
    let current = binding::production(reader)?;
    let candidate = binding::production(reader)?;
    let variant = VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?;
    let variant_digest = scalar::digest(reader)?;
    let attribution = scalar::digest(reader)?;
    let evaluation = scalar::digest(reader)?;
    let review = reader
        .read_option_tag()
        .map_err(scalar::codec)?
        .then(|| binding::review(reader))
        .transpose()?;
    let policy = scalar::digest(reader)?;
    let selection = scalar::digest(reader)?;
    let rollback = binding::production(reader)?;
    let artifact = scalar::digest(reader)?;
    PromotionProposal::from_exact_parts(
        project,
        campaign,
        current,
        candidate,
        variant,
        variant_digest,
        attribution,
        evaluation,
        review,
        policy,
        selection,
        rollback,
        artifact,
    )
}

pub(super) fn write_rollback(
    writer: &mut CanonicalWriter,
    value: &RollbackProposal,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.project_id().as_bytes()).map_err(scalar::codec)?;
    binding::write_production(writer, value.current())?;
    binding::write_production(writer, value.target())?;
    writer.write_fixed(value.target_activation().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.rollback_of().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.policy_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.compatibility_evidence_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evidence_bundle_artifact().as_bytes()).map_err(scalar::codec)
}

pub(super) fn rollback(
    reader: &mut CanonicalReader<'_>,
) -> Result<RollbackProposal, EvolutionError> {
    let project = scalar::project_id(reader).map_err(scalar::codec)?;
    let current = binding::production(reader)?;
    let target = binding::production(reader)?;
    let target_activation = ActivationId::new(reader.read_fixed().map_err(scalar::codec)?)?;
    let rollback_of = ActivationId::new(reader.read_fixed().map_err(scalar::codec)?)?;
    RollbackProposal::from_exact_parts(
        project,
        current,
        target,
        target_activation,
        rollback_of,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
    )
}

pub(super) fn write_publication(
    writer: &mut CanonicalWriter,
    value: CampaignPublication,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.artifact_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evidence_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evidence_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_u64(value.journal_position()).map_err(scalar::codec)
}

pub(super) fn publication(
    reader: &mut CanonicalReader<'_>,
) -> Result<CampaignPublication, EvolutionError> {
    CampaignPublication::new(
        scalar::digest(reader)?,
        EvidenceId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?,
        scalar::digest(reader)?,
        reader.read_u64().map_err(scalar::codec)?,
    )
}
