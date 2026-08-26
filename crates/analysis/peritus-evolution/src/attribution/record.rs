//! Immutable deterministic attribution outputs.

use crate::{
    AttributionId, ChangeManifestId, EvolutionError, EvolutionErrorKind, EvolutionLimits,
    EvolutionOperation, EvolutionRecovery, InteractionGroupId, MetricValue, VariantId,
    identity::digest_parts,
};
use peritus_eval::MetricUnavailableReason;
use peritus_types::Sha256Digest;

/// Exact reason a declared prediction could not be observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttributionUnavailable {
    /// E3 retained an explicit unavailable reason.
    Evaluation(MetricUnavailableReason),
    /// The exact predicted task was absent from the frozen report.
    TaskAbsent,
    /// The requested configured metric was absent for the task.
    MetricAbsent,
    /// E3 does not retain outcome class at the requested failure-class granularity.
    UnsupportedFailureClass,
    /// Checked completeness arithmetic could not be represented.
    Arithmetic,
}

/// One exact observed value or a visible unavailability reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricObservation {
    /// An exact integer/fixed-point value was present.
    Available(MetricValue),
    /// The value was not available for one exact reason.
    Unavailable(AttributionUnavailable),
}

/// Closed falsification result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FalsificationVerdict {
    /// Available evidence satisfied the declared relation.
    Confirmed,
    /// Available evidence contradicted the declared relation.
    Contradicted,
    /// Evidence existed but could not decide the prediction.
    Inconclusive,
    /// The declared task or configured metric was not observed.
    NotObserved,
}

/// One manifest prediction evaluated against exact E3 evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttributionEntry {
    manifest_id: ChangeManifestId,
    prediction_digest: Sha256Digest,
    observation: MetricObservation,
    verdict: FalsificationVerdict,
    mandatory: bool,
    critical: bool,
}

impl AttributionEntry {
    pub(crate) const fn new(
        manifest_id: ChangeManifestId,
        prediction_digest: Sha256Digest,
        observation: MetricObservation,
        verdict: FalsificationVerdict,
        mandatory: bool,
        critical: bool,
    ) -> Self {
        Self { manifest_id, prediction_digest, observation, verdict, mandatory, critical }
    }
    /// Returns the owning manifest identity.
    #[must_use]
    pub const fn manifest_id(self) -> ChangeManifestId {
        self.manifest_id
    }
    /// Returns the exact prediction digest.
    #[must_use]
    pub const fn prediction_digest(self) -> Sha256Digest {
        self.prediction_digest
    }
    /// Returns the exact value or unavailability reason.
    #[must_use]
    pub const fn observation(self) -> MetricObservation {
        self.observation
    }
    /// Returns the closed falsification result.
    #[must_use]
    pub const fn verdict(self) -> FalsificationVerdict {
        self.verdict
    }
    /// Returns whether the prediction is mandatory.
    #[must_use]
    pub const fn mandatory(self) -> bool {
        self.mandatory
    }
    /// Returns whether contradiction is a critical regression.
    #[must_use]
    pub const fn critical(self) -> bool {
        self.critical
    }
}

/// Complete attribution for one variant under one published evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributionRecord {
    id: AttributionId,
    variant_id: VariantId,
    evaluation_digest: Sha256Digest,
    interaction_group: Option<InteractionGroupId>,
    entries: Vec<AttributionEntry>,
    coverage_millionths: u32,
    critical_regressions: u32,
    mandatory_failures: u32,
    digest: Sha256Digest,
}

impl AttributionRecord {
    pub(crate) fn from_exact_parts(
        variant_id: VariantId,
        evaluation_digest: Sha256Digest,
        interaction_group: Option<InteractionGroupId>,
        entries: Vec<AttributionEntry>,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        if entries.is_empty()
            || entries.len() > usize::try_from(limits.attribution_entries()).unwrap_or(usize::MAX)
            || entries.windows(2).any(|pair| {
                (pair[0].manifest_id(), pair[0].prediction_digest())
                    >= (pair[1].manifest_id(), pair[1].prediction_digest())
            })
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::NonCanonical,
                EvolutionOperation::Attribute,
                EvolutionRecovery::Quarantine,
                "persisted attribution entries are empty, noncanonical, or over limit",
            ));
        }
        let decidable = entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.verdict(),
                    FalsificationVerdict::Confirmed | FalsificationVerdict::Contradicted
                )
            })
            .count();
        let coverage_millionths = u32::try_from(
            u64::try_from(decidable)
                .ok()
                .and_then(|value| value.checked_mul(1_000_000))
                .and_then(|value| value.checked_div(u64::try_from(entries.len()).ok()?))
                .ok_or_else(arithmetic)?,
        )
        .map_err(|_| arithmetic())?;
        let critical_regressions = count(&entries, |entry| {
            entry.critical() && entry.verdict() == FalsificationVerdict::Contradicted
        })?;
        let mandatory_failures = count(&entries, |entry| {
            entry.mandatory() && entry.verdict() != FalsificationVerdict::Confirmed
        })?;
        let digest = attribution_digest(
            variant_id,
            evaluation_digest,
            interaction_group,
            &entries,
            coverage_millionths,
            critical_regressions,
            mandatory_failures,
        );
        let id = AttributionId::derive(b"peritus.f0.attribution-id.v1\0", digest);
        Ok(Self {
            id,
            variant_id,
            evaluation_digest,
            interaction_group,
            entries,
            coverage_millionths,
            critical_regressions,
            mandatory_failures,
            digest,
        })
    }
    /// Returns the deterministic attribution identity.
    #[must_use]
    pub const fn id(&self) -> AttributionId {
        self.id
    }
    /// Returns the evaluated variant identity.
    #[must_use]
    pub const fn variant_id(&self) -> VariantId {
        self.variant_id
    }
    /// Returns the exact published evaluation binding digest.
    #[must_use]
    pub const fn evaluation_digest(&self) -> Sha256Digest {
        self.evaluation_digest
    }
    /// Returns the group receiving multi-change attribution, when any.
    #[must_use]
    pub const fn interaction_group(&self) -> Option<InteractionGroupId> {
        self.interaction_group
    }
    /// Borrows prediction-level falsification results.
    #[must_use]
    pub fn entries(&self) -> &[AttributionEntry] {
        &self.entries
    }
    /// Returns decidable predictions divided by all predictions in millionths.
    #[must_use]
    pub const fn coverage_millionths(&self) -> u32 {
        self.coverage_millionths
    }
    /// Returns contradicted critical predictions.
    #[must_use]
    pub const fn critical_regressions(&self) -> u32 {
        self.critical_regressions
    }
    /// Returns contradicted or unavailable mandatory predictions.
    #[must_use]
    pub const fn mandatory_failures(&self) -> u32 {
        self.mandatory_failures
    }
    /// Returns the canonical attribution digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn count(
    entries: &[AttributionEntry],
    predicate: impl Fn(&AttributionEntry) -> bool,
) -> Result<u32, EvolutionError> {
    u32::try_from(entries.iter().filter(|entry| predicate(entry)).count()).map_err(|_| arithmetic())
}

fn attribution_digest(
    variant_id: VariantId,
    evaluation_digest: Sha256Digest,
    interaction_group: Option<InteractionGroupId>,
    entries: &[AttributionEntry],
    coverage_millionths: u32,
    critical_regressions: u32,
    mandatory_failures: u32,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(variant_id.as_bytes());
    bytes.extend_from_slice(evaluation_digest.as_bytes());
    if let Some(group) = interaction_group {
        bytes.push(1);
        bytes.extend_from_slice(group.as_bytes());
    } else {
        bytes.push(0);
    }
    for entry in entries {
        bytes.extend_from_slice(entry.manifest_id().as_bytes());
        bytes.extend_from_slice(entry.prediction_digest().as_bytes());
        append_observation(&mut bytes, entry.observation());
        bytes.push(match entry.verdict() {
            FalsificationVerdict::Confirmed => 1,
            FalsificationVerdict::Contradicted => 2,
            FalsificationVerdict::Inconclusive => 3,
            FalsificationVerdict::NotObserved => 4,
        });
        bytes.push(u8::from(entry.mandatory()));
        bytes.push(u8::from(entry.critical()));
    }
    bytes.extend_from_slice(&coverage_millionths.to_be_bytes());
    bytes.extend_from_slice(&critical_regressions.to_be_bytes());
    bytes.extend_from_slice(&mandatory_failures.to_be_bytes());
    digest_parts(b"peritus.f0.attribution.v1\0", &[&bytes])
}

fn append_observation(bytes: &mut Vec<u8>, observation: MetricObservation) {
    match observation {
        MetricObservation::Available(value) => {
            bytes.push(1);
            match value {
                MetricValue::SignedMillionths(item) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&item.to_be_bytes());
                }
                MetricValue::ProbabilityMillionths(item) => {
                    bytes.push(2);
                    bytes.extend_from_slice(&item.to_be_bytes());
                }
                MetricValue::Count(item) => {
                    bytes.push(3);
                    bytes.extend_from_slice(&item.to_be_bytes());
                }
                MetricValue::Quantity(item) => {
                    bytes.push(4);
                    bytes.extend_from_slice(&item.to_be_bytes());
                }
            }
        }
        MetricObservation::Unavailable(reason) => {
            bytes.push(2);
            match reason {
                AttributionUnavailable::Evaluation(value) => {
                    bytes.push(1);
                    bytes.push(crate::binding::reason_tag(value));
                }
                AttributionUnavailable::TaskAbsent => bytes.push(2),
                AttributionUnavailable::MetricAbsent => bytes.push(3),
                AttributionUnavailable::UnsupportedFailureClass => bytes.push(4),
                AttributionUnavailable::Arithmetic => bytes.push(5),
            }
        }
    }
}

const fn arithmetic() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Arithmetic,
        EvolutionOperation::Attribute,
        EvolutionRecovery::Quarantine,
        "persisted attribution arithmetic overflowed",
    )
}
