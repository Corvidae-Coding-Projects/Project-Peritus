//! Final H4 bundle assembly from authenticated retained inputs.

use crate::{EvidenceKind, QualificationInputs, QualificationReport, QualificationVerdict};

use super::{
    OperatorError, admission::EvidenceStore, args::FinalizeInput, audit_input, binding,
    build_input, campaign_input, criterion_input, files, plan::QualificationPlan, policy_input,
};

pub(super) fn finalize(input: &FinalizeInput) -> Result<(), OperatorError> {
    let plan_bytes = files::read_bounded_regular(&input.plan, "H4 qualification plan")?;
    let plan: QualificationPlan = serde_json::from_slice(&plan_bytes)?;
    if plan.schema_version != 1 {
        return Err(OperatorError::integrity("H4 qualification plan schema_version must be 1"));
    }
    let metadata = std::fs::symlink_metadata(&input.evidence_root).map_err(|source| {
        OperatorError::io("inspect evidence root", &input.evidence_root, &source)
    })?;
    if !metadata.is_dir() {
        return Err(OperatorError::integrity("evidence root must be an existing directory"));
    }
    let binding = binding::from_spec(&plan.binding)?;
    let evidence = EvidenceStore::verify_all(&binding, &input.evidence_root, &plan.evidence)?;
    let builds = build_input::assemble(
        &binding,
        &input.evidence_root,
        &plan.primary_build,
        &plan.independent_build,
    )?;
    let criteria = criterion_input::assemble(&plan.criteria, &evidence)?;
    let collection = campaign_input::assemble(&binding, &plan.campaigns, &evidence)?;
    let audit = audit_input::assemble(&binding, &input.evidence_root, &evidence, &plan.audit)?;
    let policy = policy_input::assemble(
        &binding,
        &builds.inventory,
        &audit.manifest,
        &criteria,
        &audit.final_audit,
        &evidence,
        plan.evaluated_at,
    )?;
    let mut inputs = QualificationInputs::new(binding)
        .collection_run(collection.clone())
        .artifact_inventory(builds.inventory.clone())
        .reproducibility(builds.comparison.clone())
        .criterion_map(criteria.clone())
        .evidence_manifest(audit.manifest.clone())
        .final_audit(audit.final_audit.clone());
    for record in evidence.records().filter(|record| {
        !EvidenceKind::fresh_subject_campaigns().contains(&record.evidence_reference().kind())
    }) {
        inputs = inputs.evidence(record.clone());
    }
    let report = QualificationReport::evaluate(&inputs, &policy)?;
    let entries = [
        ("qualification-report.json", report.canonical_json()?),
        ("evidence-manifest.json", audit.manifest.canonical_json()?),
        ("artifact-inventory.json", builds.inventory.canonical_json()?),
        ("reproducibility-comparison.json", builds.comparison.canonical_json()?),
        ("criterion-map.json", criteria.canonical_json()?),
        ("collection-run.json", serde_json::to_vec(&collection)?),
        ("final-audit.json", serde_json::to_vec(&audit.final_audit)?),
    ];
    files::publish_bundle(&input.output, &entries)?;
    if report.verdict() == QualificationVerdict::Ready {
        Ok(())
    } else {
        Err(OperatorError::not_ready(format!(
            "final H4 report retained with {} blocker(s) in {}",
            report.blockers().len(),
            input.output.display()
        )))
    }
}
