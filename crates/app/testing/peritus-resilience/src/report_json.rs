//! Stable human-readable JSON projection of canonical H1 reports.

mod codes;

use serde_json::{Value, json};

use crate::{
    CaseStatus, CleanupObservation, DisruptionObservation, QualificationReport,
    RecoveryObservation, ScenarioFailure, ScenarioReport, SuiteFailure,
};

/// Renders one complete H1 report as newline-terminated UTF-8 JSON.
///
/// The report's canonical evidence digest remains the authoritative identity. This projection
/// retains the direct observations and stable failure codes needed by operators and H4.
///
/// # Errors
///
/// Returns the underlying JSON encoding failure.
pub fn render_report_json(report: &QualificationReport) -> Result<Vec<u8>, serde_json::Error> {
    let config = report.config();
    let retries = config.retries();
    let resources = config.resources();
    let subject = report.subject().map(|subject| {
        json!({
            "id": subject.id().as_str(),
            "implementation": subject.implementation().as_str(),
            "build_sha256": subject.build_digest().to_string(),
        })
    });
    let summary = report.summary();
    let mut bytes = serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "profile": codes::catalog_profile(report.profile()),
        "subject": subject,
        "limits": {
            "max_scenarios": config.max_scenarios(),
            "max_milestones_per_scenario": config.max_milestones_per_scenario(),
            "retries": {
                "provider": retries.provider(),
                "tool": retries.tool(),
                "worker": retries.worker(),
                "reconciliation": retries.reconciliation(),
            },
            "resources": {
                "events": resources.events(),
                "evidence_bytes": resources.evidence_bytes(),
                "owned_processes": resources.owned_processes(),
                "cleanup_steps": resources.cleanup_steps(),
                "logical_ticks": resources.logical_ticks(),
            },
        },
        "summary": {
            "total": summary.total(),
            "passed": summary.passed(),
            "failed": summary.failed(),
            "not_executed": summary.not_executed(),
        },
        "verdict": codes::verdict(report.verdict()),
        "suite_failure": report.suite_failure().map(suite_failure),
        "cases": report.cases().iter().map(scenario_report).collect::<Vec<_>>(),
        "evidence_sha256": report.evidence_digest().to_string(),
    }))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn scenario_report(report: &ScenarioReport) -> Value {
    let scenario = report.scenario();
    json!({
        "scenario": {
            "id": scenario.id().as_str(),
            "title": scenario.title().as_str(),
            "fault": codes::fault(scenario.fault()),
            "expected_recovery": codes::recovery(scenario.expected_recovery()),
        },
        "status": match report.status() {
            CaseStatus::NotExecuted => "not-executed",
            CaseStatus::Failed => "failed",
            CaseStatus::Passed => "passed",
        },
        "preparation": report.preparation().map(|observation| json!({
            "scenario_id": observation.scenario_id().as_str(),
            "terminal": codes::terminal(observation.terminal()),
            "journal_head_sha256": observation.journal_head().to_string(),
        })),
        "disruption": report.disruption().map(disruption),
        "recovery": report.recovery().map(recovery),
        "cleanup": report.cleanup().map(cleanup),
        "failures": report.failures().iter().map(scenario_failure).collect::<Vec<_>>(),
    })
}

fn disruption(observation: &DisruptionObservation) -> Value {
    json!({
        "scenario_id": observation.scenario_id().as_str(),
        "fault": codes::fault(observation.fault()),
        "reached": observation.reached(),
    })
}

fn recovery(observation: &RecoveryObservation) -> Value {
    let acceptance = observation.acceptance();
    let corruption = observation.corruption();
    let ownership = observation.ownership();
    let retries = observation.retries();
    let resources = observation.resources();
    json!({
        "scenario_id": observation.scenario_id().as_str(),
        "outcome": codes::recovery(observation.outcome()),
        "acceptance": {
            "terminal": codes::terminal(acceptance.terminal()),
            "revision_bound": acceptance.revision_bound(),
            "evidence_current": acceptance.evidence_current(),
        },
        "journal": codes::journal(observation.journal()),
        "artifacts": codes::artifacts(observation.artifacts()),
        "projection": codes::projection(observation.projection()),
        "corruption": {
            "detected": corruption.detected().map(codes::corrupt_target),
            "mutation_admitted": corruption.mutation_admitted(),
        },
        "ownership": {
            "scan_completed": ownership.scan_completed(),
            "discovered": ownership.discovered(),
            "resumed": ownership.resumed(),
            "failed": ownership.failed(),
            "indeterminate": ownership.indeterminate(),
            "unaccounted": ownership.unaccounted(),
            "orphan_candidates_detected": ownership.orphan_candidates_detected(),
            "orphans_remaining": ownership.orphans_remaining(),
        },
        "retries": {
            "provider": retries.provider(),
            "tool": retries.tool(),
            "worker": retries.worker(),
            "reconciliation": retries.reconciliation(),
        },
        "resources": {
            "events": resources.events(),
            "evidence_bytes": resources.evidence_bytes(),
            "peak_owned_processes": resources.peak_owned_processes(),
            "cleanup_steps": resources.cleanup_steps(),
            "logical_ticks": resources.logical_ticks(),
        },
        "temporary_objects": observation.temporary_objects(),
        "evidence": observation.evidence().iter().map(|anchor| json!({
            "kind": codes::evidence_kind(anchor.kind()),
            "id": anchor.id().as_str(),
            "sha256": anchor.digest().to_string(),
        })).collect::<Vec<_>>(),
        "milestones": observation.milestones().iter().map(|milestone| json!({
            "sequence": milestone.sequence(),
            "kind": codes::milestone(milestone.kind()),
            "detail": milestone.detail().as_str(),
        })).collect::<Vec<_>>(),
    })
}

fn cleanup(observation: CleanupObservation) -> Value {
    json!({
        "resources_released": observation.resources_released(),
        "owned_work_remaining": observation.owned_work_remaining(),
        "cleanup_steps": observation.cleanup_steps(),
    })
}

fn scenario_failure(failure: &ScenarioFailure) -> Value {
    match failure {
        ScenarioFailure::Subject { phase, error } => json!({
            "kind": "subject",
            "phase": codes::failure_phase(*phase),
            "code": codes::subject_error(error.code()),
            "context": error.context().as_str(),
            "retryable": error.retryable(),
        }),
        ScenarioFailure::Panic(panic) => json!({
            "kind": "panic",
            "phase": codes::failure_phase(panic.phase()),
        }),
        ScenarioFailure::Contract(violation) => json!({
            "kind": "contract",
            "code": codes::contract_violation(violation),
        }),
    }
}

fn suite_failure(failure: &SuiteFailure) -> Value {
    match failure {
        SuiteFailure::SubjectDescriptorPanic(panic) => json!({
            "code": "subject-descriptor-panic",
            "phase": codes::failure_phase(panic.phase()),
        }),
        SuiteFailure::CatalogExceedsConfiguration { actual, maximum } => json!({
            "code": "catalog-exceeds-configuration",
            "actual": actual,
            "maximum": maximum,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::render_report_json;
    use crate::{CatalogProfile, QualificationConfig, QualificationReport, SuiteFailure};

    #[test]
    fn projection_retains_verdict_counts_failure_and_canonical_digest() {
        let report = QualificationReport::invalid(
            QualificationConfig::default(),
            CatalogProfile::H1Production,
            None,
            SuiteFailure::CatalogExceedsConfiguration { actual: 65, maximum: 64 },
        );
        let bytes = render_report_json(&report).expect("render report");
        assert!(bytes.ends_with(b"\n"));
        let document: serde_json::Value = serde_json::from_slice(&bytes).expect("parse report");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["verdict"], "not-ready-suite-failure");
        assert_eq!(document["summary"]["total"], 0);
        assert_eq!(document["suite_failure"]["code"], "catalog-exceeds-configuration");
        assert_eq!(
            document["evidence_sha256"],
            serde_json::Value::String(report.evidence_digest().to_string())
        );
    }
}
