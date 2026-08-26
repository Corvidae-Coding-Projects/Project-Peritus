//! Closed campaign command/event semantic payload codec.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

use crate::{CampaignCommandKind, EvolutionError, EvolutionLimits, VariantId};

use super::{super::scalar, attribution, binding, change, evaluation, proposal, selection};

pub(crate) fn encode_kind(kind: &CampaignCommandKind) -> Result<Vec<u8>, EvolutionError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    match kind {
        CampaignCommandKind::CreateCampaign { project_id, baseline, policy, limits } => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_fixed(project_id.as_bytes()).map_err(scalar::codec)?;
            binding::write_production(&mut writer, *baseline)?;
            binding::write_limits(&mut writer, *limits)?;
            binding::write_policy(&mut writer, policy)?;
        }
        CampaignCommandKind::FreezeCampaign => writer.write_u8(2).map_err(scalar::codec)?,
        CampaignCommandKind::RecordBaselineEvidence { artifact_digest, evidence_digest } => {
            writer.write_u8(3).map_err(scalar::codec)?;
            writer.write_fixed(artifact_digest.as_bytes()).map_err(scalar::codec)?;
            writer.write_fixed(evidence_digest.as_bytes()).map_err(scalar::codec)?;
        }
        CampaignCommandKind::SubmitDiagnosis(value) => {
            writer.write_u8(4).map_err(scalar::codec)?;
            binding::write_diagnosis(&mut writer, value)?;
        }
        CampaignCommandKind::AdmitChangeManifest(value) => {
            writer.write_u8(5).map_err(scalar::codec)?;
            change::write_manifest(&mut writer, value)?;
        }
        CampaignCommandKind::AdmitVariant(value) => {
            writer.write_u8(6).map_err(scalar::codec)?;
            change::write_variant(&mut writer, value)?;
        }
        CampaignCommandKind::AdmitEvaluation { variant_id, evidence } => {
            writer.write_u8(7).map_err(scalar::codec)?;
            writer.write_fixed(variant_id.as_bytes()).map_err(scalar::codec)?;
            evaluation::write(&mut writer, evidence)?;
        }
        CampaignCommandKind::CompleteAttribution { attribution: value, assessment } => {
            writer.write_u8(8).map_err(scalar::codec)?;
            attribution::write(&mut writer, value)?;
            selection::write_assessment(&mut writer, assessment)?;
        }
        CampaignCommandKind::RecordSelection(value) => {
            writer.write_u8(9).map_err(scalar::codec)?;
            selection::write_selection(&mut writer, value)?;
        }
        CampaignCommandKind::RequestPromotion(value) => {
            writer.write_u8(10).map_err(scalar::codec)?;
            proposal::write_promotion(&mut writer, value)?;
        }
        CampaignCommandKind::ActivatePromotion { activation_digest } => {
            writer.write_u8(11).map_err(scalar::codec)?;
            writer.write_fixed(activation_digest.as_bytes()).map_err(scalar::codec)?;
        }
        CampaignCommandKind::RecordPublication(value) => {
            writer.write_u8(12).map_err(scalar::codec)?;
            proposal::write_publication(&mut writer, *value)?;
        }
        CampaignCommandKind::CancelCampaign { reason_digest } => {
            writer.write_u8(13).map_err(scalar::codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(scalar::codec)?;
        }
        CampaignCommandKind::FailCampaign { reason_digest } => {
            writer.write_u8(14).map_err(scalar::codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(scalar::codec)?;
        }
    }
    Ok(writer.into_bytes())
}

pub(crate) fn decode_kind(bytes: &[u8]) -> Result<CampaignCommandKind, EvolutionError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let compiled = EvolutionLimits::compiled();
    let kind = match reader.read_u8().map_err(scalar::codec)? {
        1 => {
            let project_id = scalar::project_id(&mut reader).map_err(scalar::codec)?;
            let baseline = binding::production(&mut reader)?;
            let limits = binding::limits(&mut reader)?;
            let policy = binding::policy(&mut reader, limits)?;
            CampaignCommandKind::CreateCampaign { project_id, baseline, policy, limits }
        }
        2 => CampaignCommandKind::FreezeCampaign,
        3 => CampaignCommandKind::RecordBaselineEvidence {
            artifact_digest: scalar::digest(&mut reader)?,
            evidence_digest: scalar::digest(&mut reader)?,
        },
        4 => CampaignCommandKind::SubmitDiagnosis(binding::diagnosis(&mut reader, compiled)?),
        5 => CampaignCommandKind::AdmitChangeManifest(change::manifest(&mut reader, compiled)?),
        6 => CampaignCommandKind::AdmitVariant(change::variant(&mut reader, compiled)?),
        7 => CampaignCommandKind::AdmitEvaluation {
            variant_id: VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?,
            evidence: evaluation::read(&mut reader)?,
        },
        8 => CampaignCommandKind::CompleteAttribution {
            attribution: attribution::read(&mut reader, compiled)?,
            assessment: selection::assessment(&mut reader, compiled)?,
        },
        9 => CampaignCommandKind::RecordSelection(selection::selection(&mut reader)?),
        10 => CampaignCommandKind::RequestPromotion(proposal::promotion(&mut reader)?),
        11 => CampaignCommandKind::ActivatePromotion {
            activation_digest: scalar::digest(&mut reader)?,
        },
        12 => CampaignCommandKind::RecordPublication(proposal::publication(&mut reader)?),
        13 => CampaignCommandKind::CancelCampaign { reason_digest: scalar::digest(&mut reader)? },
        14 => CampaignCommandKind::FailCampaign { reason_digest: scalar::digest(&mut reader)? },
        _ => return Err(scalar::protocol()),
    };
    reader.finish().map_err(scalar::codec)?;
    Ok(kind)
}
