//! Strict JSON inputs for final H4 evidence reduction.

use serde::{Deserialize, Serialize};

use peritus_release_artifacts::ArtifactRole;

use crate::{EvidenceDisposition, EvidenceKind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QualificationPlan {
    pub schema_version: u32,
    pub binding: BindingSpec,
    pub evidence: Vec<EvidenceSpec>,
    pub campaigns: Vec<CampaignSpec>,
    pub primary_build: BuildSpec,
    pub independent_build: BuildSpec,
    pub criteria: Vec<CriterionSpec>,
    pub audit: AuditSpec,
    pub evaluated_at: u64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BindingSpec {
    pub candidate_commit: String,
    pub version: String,
    pub toolchain: String,
    pub platform: String,
    pub source_tree_digest: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceSpec {
    pub kind: EvidenceKind,
    pub disposition: EvidenceDisposition,
    pub path: String,
    pub key_id: String,
    pub public_key_path: String,
    pub signature_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CampaignSpec {
    pub schema_version: u32,
    pub kind: EvidenceKind,
    pub subject_id: String,
    pub cleanup: CleanupSpec,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_field_names,
    reason = "cleanup interchange names residual resources explicitly"
)]
pub(super) struct CleanupSpec {
    pub remaining_processes: u32,
    pub remaining_mounts: u32,
    pub remaining_worktrees: u32,
    pub remaining_temporary_paths: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BuildSpec {
    pub builder_id: String,
    pub artifacts: Vec<ArtifactSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactSpec {
    pub path: String,
    pub source_path: String,
    pub media_type: String,
    pub roles: Vec<ArtifactRole>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CriterionSpec {
    pub criterion: String,
    pub evidence: Vec<EvidenceSelector>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceSelector {
    pub kind: EvidenceKind,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditSpec {
    pub auditor: String,
    pub contributors: Vec<String>,
    pub findings: Vec<AuditFindingSpec>,
    pub path: String,
    pub key_id: String,
    pub public_key_path: String,
    pub signature_path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditFindingSpec {
    pub id: String,
    pub severity: AuditSeverity,
    pub summary: String,
    pub disposition: AuditDispositionSpec,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum AuditSeverity {
    Informational,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum AuditDispositionSpec {
    Open,
    Closed { evidence: EvidenceSelector },
    RiskAccepted { evidence: EvidenceSelector },
}

#[cfg(test)]
mod tests {
    use super::QualificationPlan;
    use crate::{AcceptanceCriterion, EvidenceDisposition, EvidenceKind};

    const TEMPLATE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../release/templates/release-inputs.template.json"
    ));

    #[test]
    fn checked_operator_template_matches_the_strict_plan_and_remains_not_satisfied() {
        let plan: QualificationPlan = serde_json::from_str(TEMPLATE).expect("H4 plan template");

        assert_eq!(plan.schema_version, 1);
        assert_eq!(plan.evidence.len(), EvidenceKind::required_signed_inputs().len());
        assert_eq!(plan.campaigns.len(), EvidenceKind::fresh_subject_campaigns().len());
        assert_eq!(plan.criteria.len(), AcceptanceCriterion::all().len());
        assert_ne!(plan.primary_build.builder_id, plan.independent_build.builder_id);
        assert!(
            plan.evidence
                .iter()
                .all(|evidence| evidence.disposition == EvidenceDisposition::NotSatisfied)
        );
    }
}
