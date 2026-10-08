//! Canonical F0 limits, E1, E2, policy, review, and authority bridges.

use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_debugger::{ClaimId, DebuggerJobId, PatternId, ReportId, SelectionManifestId};
use peritus_types::{EvidenceId, Generation, RevisionNumber, RunId, WorkspaceId};

use crate::{
    ActivationAuthorization, DiagnosisCitation, EvolutionError, EvolutionLimits,
    InstalledSnapshotBinding, MeasurementRequirement, Objective, ProductionHarnessBinding,
    PromotionPolicy, PromotionPolicyBinding, PromotionReviewEvidence, PromotionThresholds,
    PublishedDebuggerEvidence,
};

use super::super::scalar;

pub(super) fn write_limits(
    writer: &mut CanonicalWriter,
    value: EvolutionLimits,
) -> Result<(), EvolutionError> {
    writer.write_u16(value.manifests()).map_err(scalar::codec)?;
    writer.write_u16(value.variants()).map_err(scalar::codec)?;
    writer.write_u16(value.citations_per_manifest()).map_err(scalar::codec)?;
    writer.write_u16(value.deltas_per_manifest()).map_err(scalar::codec)?;
    writer.write_u16(value.predictions_per_manifest()).map_err(scalar::codec)?;
    writer.write_u32(value.attribution_entries()).map_err(scalar::codec)?;
    writer.write_u16(value.criteria()).map_err(scalar::codec)?;
    writer.write_u32(value.text_bytes()).map_err(scalar::codec)?;
    writer.write_u16(value.activation_history()).map_err(scalar::codec)
}

pub(super) fn limits(reader: &mut CanonicalReader<'_>) -> Result<EvolutionLimits, EvolutionError> {
    EvolutionLimits::new(
        reader.read_u16().map_err(scalar::codec)?,
        reader.read_u16().map_err(scalar::codec)?,
        reader.read_u16().map_err(scalar::codec)?,
        reader.read_u16().map_err(scalar::codec)?,
        reader.read_u16().map_err(scalar::codec)?,
        reader.read_u32().map_err(scalar::codec)?,
        reader.read_u16().map_err(scalar::codec)?,
        reader.read_u32().map_err(scalar::codec)?,
        reader.read_u16().map_err(scalar::codec)?,
    )
}

pub(super) fn write_production(
    writer: &mut CanonicalWriter,
    value: ProductionHarnessBinding,
) -> Result<(), EvolutionError> {
    scalar::write_revision(writer, value.revision())?;
    scalar::write_harness_revision(writer, value.harness_revision())?;
    writer.write_fixed(value.materialization_receipt_digest().as_bytes()).map_err(scalar::codec)?;
    let snapshot = value.installed_snapshot();
    writer.write_fixed(snapshot.workspace_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_u64(snapshot.generation().get()).map_err(scalar::codec)?;
    writer.write_u64(snapshot.revision().get()).map_err(scalar::codec)?;
    let (commit_format, commit) = snapshot.commit();
    writer.write_u8(commit_format).map_err(scalar::codec)?;
    writer.write_fixed(&commit).map_err(scalar::codec)?;
    let (tree_format, tree) = snapshot.tree();
    writer.write_u8(tree_format).map_err(scalar::codec)?;
    writer.write_fixed(&tree).map_err(scalar::codec)
}

pub(super) fn production(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductionHarnessBinding, EvolutionError> {
    let revision = scalar::revision(reader)?;
    let harness_revision = scalar::harness_revision(reader)?;
    let receipt = scalar::digest(reader)?;
    let workspace =
        WorkspaceId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?;
    let generation =
        Generation::new(reader.read_u64().map_err(scalar::codec)?).map_err(scalar::domain)?;
    let number =
        RevisionNumber::new(reader.read_u64().map_err(scalar::codec)?).map_err(scalar::domain)?;
    let commit = object(reader)?;
    let tree = object(reader)?;
    if revision.workspace_id() != workspace
        || revision.workspace_generation() != generation
        || revision.workspace_revision() != number
        || revision.harness_id() != harness_revision.harness_id()
    {
        return Err(scalar::protocol());
    }
    let snapshot =
        InstalledSnapshotBinding::from_replay_parts(workspace, generation, number, commit, tree);
    Ok(ProductionHarnessBinding::from_exact_parts(revision, harness_revision, receipt, snapshot))
}

fn object(reader: &mut CanonicalReader<'_>) -> Result<(u8, [u8; 32]), EvolutionError> {
    let format = reader.read_u8().map_err(scalar::codec)?;
    let bytes: [u8; 32] = reader.read_fixed().map_err(scalar::codec)?;
    if !matches!(format, 1 | 2) || (format == 1 && bytes[20..].iter().any(|value| *value != 0)) {
        return Err(scalar::protocol());
    }
    Ok((format, bytes))
}

pub(super) fn write_policy(
    writer: &mut CanonicalWriter,
    value: &PromotionPolicyBinding,
) -> Result<(), EvolutionError> {
    scalar::write_harness_revision(writer, value.production_revision())?;
    writer.write_str(value.component_id().as_str()).map_err(scalar::codec)?;
    writer.write_fixed(value.component_digest().as_bytes()).map_err(scalar::codec)?;
    let policy = value.policy();
    let thresholds = policy.thresholds();
    if policy.has_explicit_measurements() {
        writer.write_fixed(&i32::MIN.to_be_bytes()).map_err(scalar::codec)?;
        writer.write_u8(2).map_err(scalar::codec)?;
    }
    writer
        .write_fixed(&thresholds.minimum_paired_lower_millionths().to_be_bytes())
        .map_err(scalar::codec)?;
    writer.write_u32(thresholds.maximum_critical_regressions()).map_err(scalar::codec)?;
    writer.write_u32(thresholds.maximum_safety_failures()).map_err(scalar::codec)?;
    writer.write_u32(thresholds.minimum_reliability_lower_millionths()).map_err(scalar::codec)?;
    writer
        .write_u32(thresholds.minimum_attribution_coverage_millionths())
        .map_err(scalar::codec)?;
    for requirement in [
        thresholds.measurement(crate::PromotionMeasurement::LatencyP95),
        thresholds.measurement(crate::PromotionMeasurement::CostMean),
        thresholds.measurement(crate::PromotionMeasurement::InputTokensMean),
        thresholds.measurement(crate::PromotionMeasurement::OutputTokensMean),
    ] {
        if policy.has_explicit_measurements() {
            write_measurement_requirement(writer, requirement)?;
        } else {
            writer
                .write_u64(requirement.maximum().ok_or_else(scalar::protocol)?)
                .map_err(scalar::codec)?;
        }
    }
    writer.write_bool(thresholds.require_complete_trace()).map_err(scalar::codec)?;
    writer.write_bool(thresholds.require_complete_teardown()).map_err(scalar::codec)?;
    writer.write_collection_len(policy.objectives().len()).map_err(scalar::codec)?;
    for objective in policy.objectives() {
        writer.write_u8(objective.tag()).map_err(scalar::codec)?;
    }
    writer.write_collection_len(policy.review_required_kinds().len()).map_err(scalar::codec)?;
    for kind in policy.review_required_kinds() {
        writer.write_u8(kind.tag()).map_err(scalar::codec)?;
    }
    writer.write_bool(policy.allow_cross_lineage()).map_err(scalar::codec)?;
    writer.write_u16(policy.maximum_variants()).map_err(scalar::codec)
}

pub(super) fn policy(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<PromotionPolicyBinding, EvolutionError> {
    let production_revision = scalar::harness_revision(reader)?;
    let component_id = scalar::component_id(reader)?;
    let component_digest = scalar::digest(reader)?;
    let marker_or_minimum = i32::from_be_bytes(reader.read_fixed().map_err(scalar::codec)?);
    let explicit_measurements = marker_or_minimum == i32::MIN;
    let minimum_paired_lower_millionths = if explicit_measurements {
        if reader.read_u8().map_err(scalar::codec)? != 2 {
            return Err(scalar::protocol());
        }
        i32::from_be_bytes(reader.read_fixed().map_err(scalar::codec)?)
    } else {
        marker_or_minimum
    };
    let maximum_critical_regressions = reader.read_u32().map_err(scalar::codec)?;
    let maximum_safety_failures = reader.read_u32().map_err(scalar::codec)?;
    let minimum_reliability_lower_millionths = reader.read_u32().map_err(scalar::codec)?;
    let minimum_attribution_coverage_millionths = reader.read_u32().map_err(scalar::codec)?;
    let measurements = if explicit_measurements {
        [
            measurement_requirement(reader)?,
            measurement_requirement(reader)?,
            measurement_requirement(reader)?,
            measurement_requirement(reader)?,
        ]
    } else {
        [
            MeasurementRequirement::RequiredMaximum(
                reader.read_u64().map_err(scalar::codec)?,
            ),
            MeasurementRequirement::RequiredMaximum(
                reader.read_u64().map_err(scalar::codec)?,
            ),
            MeasurementRequirement::RequiredMaximum(
                reader.read_u64().map_err(scalar::codec)?,
            ),
            MeasurementRequirement::RequiredMaximum(
                reader.read_u64().map_err(scalar::codec)?,
            ),
        ]
    };
    let require_complete_trace = reader.read_bool().map_err(scalar::codec)?;
    let require_complete_teardown = reader.read_bool().map_err(scalar::codec)?;
    let thresholds = if explicit_measurements {
        PromotionThresholds::new_with_measurements(
            minimum_paired_lower_millionths,
            maximum_critical_regressions,
            maximum_safety_failures,
            minimum_reliability_lower_millionths,
            minimum_attribution_coverage_millionths,
            measurements[0],
            measurements[1],
            measurements[2],
            measurements[3],
            require_complete_trace,
            require_complete_teardown,
        )?
    } else {
        PromotionThresholds::new(
            minimum_paired_lower_millionths,
            maximum_critical_regressions,
            maximum_safety_failures,
            minimum_reliability_lower_millionths,
            minimum_attribution_coverage_millionths,
            measurements[0].maximum().ok_or_else(scalar::protocol)?,
            measurements[1].maximum().ok_or_else(scalar::protocol)?,
            measurements[2].maximum().ok_or_else(scalar::protocol)?,
            measurements[3].maximum().ok_or_else(scalar::protocol)?,
            require_complete_trace,
            require_complete_teardown,
        )?
    };
    let objective_count = reader.read_collection_len(1).map_err(scalar::codec)?;
    let mut objectives = reader.reserve_collection(objective_count).map_err(scalar::codec)?;
    for _ in 0..objective_count {
        objectives.push(objective(reader.read_u8().map_err(scalar::codec)?)?);
    }
    let review_count = reader.read_collection_len(1).map_err(scalar::codec)?;
    let mut review_kinds = reader.reserve_collection(review_count).map_err(scalar::codec)?;
    for _ in 0..review_count {
        review_kinds.push(scalar::component_kind(reader)?);
    }
    let policy = PromotionPolicy::new(
        thresholds,
        objectives,
        review_kinds,
        reader.read_bool().map_err(scalar::codec)?,
        reader.read_u16().map_err(scalar::codec)?,
        limits,
    )?;
    PromotionPolicyBinding::from_exact_parts(
        production_revision,
        component_id,
        component_digest,
        policy,
    )
}

fn write_measurement_requirement(
    writer: &mut CanonicalWriter,
    requirement: MeasurementRequirement,
) -> Result<(), EvolutionError> {
    match requirement {
        MeasurementRequirement::Optional => writer.write_u8(0).map_err(scalar::codec),
        MeasurementRequirement::RequiredMaximum(maximum) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_u64(maximum).map_err(scalar::codec)
        }
    }
}

fn measurement_requirement(
    reader: &mut CanonicalReader<'_>,
) -> Result<MeasurementRequirement, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        0 => Ok(MeasurementRequirement::Optional),
        1 => Ok(MeasurementRequirement::RequiredMaximum(
            reader.read_u64().map_err(scalar::codec)?,
        )),
        _ => Err(scalar::protocol()),
    }
}

const fn objective(tag: u8) -> Result<Objective, EvolutionError> {
    match tag {
        1 => Ok(Objective::PairedCorrectness),
        2 => Ok(Objective::CriticalRegressions),
        3 => Ok(Objective::SafetyFailures),
        4 => Ok(Objective::Reliability),
        5 => Ok(Objective::Latency),
        6 => Ok(Objective::Cost),
        7 => Ok(Objective::InputTokens),
        8 => Ok(Objective::OutputTokens),
        9 => Ok(Objective::AttributionCoverage),
        _ => Err(scalar::protocol()),
    }
}

pub(super) fn write_diagnosis(
    writer: &mut CanonicalWriter,
    value: &PublishedDebuggerEvidence,
) -> Result<(), EvolutionError> {
    scalar::write_revision(writer, value.revision())?;
    writer.write_fixed(value.job_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.report_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.report_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.manifest_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.manifest_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.query_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.artifact_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_u64(value.artifact_size()).map_err(scalar::codec)?;
    writer.write_fixed(value.evidence_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_u64(value.journal_position()).map_err(scalar::codec)?;
    writer.write_collection_len(value.citations().len()).map_err(scalar::codec)?;
    for citation in value.citations() {
        match citation {
            DiagnosisCitation::Claim(id) => {
                writer.write_u8(1).map_err(scalar::codec)?;
                writer.write_fixed(id.as_bytes()).map_err(scalar::codec)?;
            }
            DiagnosisCitation::Pattern(id) => {
                writer.write_u8(2).map_err(scalar::codec)?;
                writer.write_fixed(id.as_bytes()).map_err(scalar::codec)?;
            }
            DiagnosisCitation::Component { pattern_id, component_id } => {
                writer.write_u8(3).map_err(scalar::codec)?;
                writer.write_fixed(pattern_id.as_bytes()).map_err(scalar::codec)?;
                writer.write_str(component_id.as_str()).map_err(scalar::codec)?;
            }
        }
    }
    Ok(())
}

pub(super) fn diagnosis(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<PublishedDebuggerEvidence, EvolutionError> {
    let revision = scalar::revision(reader)?;
    let job =
        DebuggerJobId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?;
    let report =
        ReportId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?;
    let report_digest = scalar::digest(reader)?;
    let manifest = SelectionManifestId::new(reader.read_fixed().map_err(scalar::codec)?)
        .map_err(scalar::domain)?;
    let manifest_digest = scalar::digest(reader)?;
    let query_digest = scalar::digest(reader)?;
    let artifact_digest = scalar::digest(reader)?;
    let artifact_size = reader.read_u64().map_err(scalar::codec)?;
    let evidence =
        EvidenceId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?;
    let position = reader.read_u64().map_err(scalar::codec)?;
    let length = reader.read_collection_len(1 + 16).map_err(scalar::codec)?;
    let mut citations = reader.reserve_collection(length).map_err(scalar::codec)?;
    for _ in 0..length {
        citations.push(match reader.read_u8().map_err(scalar::codec)? {
            1 => DiagnosisCitation::Claim(
                ClaimId::new(reader.read_fixed().map_err(scalar::codec)?)
                    .map_err(scalar::domain)?,
            ),
            2 => DiagnosisCitation::Pattern(
                PatternId::new(reader.read_fixed().map_err(scalar::codec)?)
                    .map_err(scalar::domain)?,
            ),
            3 => DiagnosisCitation::Component {
                pattern_id: PatternId::new(reader.read_fixed().map_err(scalar::codec)?)
                    .map_err(scalar::domain)?,
                component_id: scalar::component_id(reader)?,
            },
            _ => return Err(scalar::protocol()),
        });
    }
    PublishedDebuggerEvidence::from_exact_parts(
        revision,
        job,
        report,
        report_digest,
        manifest,
        manifest_digest,
        query_digest,
        artifact_digest,
        artifact_size,
        evidence,
        position,
        citations,
        limits,
    )
}

pub(super) fn write_review(
    writer: &mut CanonicalWriter,
    value: PromotionReviewEvidence,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.run_id().as_bytes()).map_err(scalar::codec)?;
    for digest in [
        value.binding_digest(),
        value.state_digest(),
        value.terminal_digest(),
        value.candidate_revision_digest(),
        value.tree_digest(),
    ] {
        writer.write_fixed(digest.as_bytes()).map_err(scalar::codec)?;
    }
    Ok(())
}

pub(super) fn review(
    reader: &mut CanonicalReader<'_>,
) -> Result<PromotionReviewEvidence, EvolutionError> {
    Ok(PromotionReviewEvidence::from_exact_parts(
        RunId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
    ))
}

pub(super) fn write_authorization(
    writer: &mut CanonicalWriter,
    value: ActivationAuthorization,
) -> Result<(), EvolutionError> {
    for digest in [
        value.action_digest(),
        value.dispatch_digest(),
        value.capability_use_digest(),
        value.approval_use_digest(),
        value.authority_digest(),
    ] {
        writer.write_fixed(digest.as_bytes()).map_err(scalar::codec)?;
    }
    Ok(())
}

pub(super) fn authorization(
    reader: &mut CanonicalReader<'_>,
) -> Result<ActivationAuthorization, EvolutionError> {
    Ok(ActivationAuthorization::new(
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
    ))
}
