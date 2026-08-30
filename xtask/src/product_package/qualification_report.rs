//! Bounded diagnostic projection of a retained native H2 report.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::XtaskError;

const MAX_REPORT_BYTES: u64 = 4 * 1024 * 1024;

pub(super) fn not_ready_reasons(path: &Path) -> Result<String, XtaskError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| XtaskError::io("inspect retained native H2 report at", path, error))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_REPORT_BYTES {
        return Err(XtaskError::metadata(format!(
            "retained native H2 report is not a nonempty regular file within {} bytes: {}",
            MAX_REPORT_BYTES,
            path.display()
        )));
    }
    let bytes = fs::read(path)
        .map_err(|error| XtaskError::io("read retained native H2 report at", path, error))?;
    parse_not_ready_reasons(&bytes)
}

fn parse_not_ready_reasons(bytes: &[u8]) -> Result<String, XtaskError> {
    let document: ReportSummary = serde_json::from_slice(bytes).map_err(|error| {
        XtaskError::metadata(format!("retained native H2 report could not be decoded: {error}"))
    })?;
    match document.verdict {
        Verdict::NotReady { reasons } if !reasons.is_empty() => Ok(reasons
            .into_iter()
            .map(|reason| format!("{}:{}", reason.kind, reason.scenario_id))
            .collect::<Vec<_>>()
            .join(", ")),
        Verdict::NotReady { .. } => {
            Err(XtaskError::metadata("native H2 NotReady report omitted its reasons"))
        }
        Verdict::Ready => Err(XtaskError::metadata(
            "native H2 process failed even though its retained report claimed Ready",
        )),
    }
}

#[derive(Deserialize)]
struct ReportSummary {
    verdict: Verdict,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
enum Verdict {
    Ready,
    NotReady { reasons: Vec<Reason> },
}

#[derive(Deserialize)]
struct Reason {
    kind: String,
    scenario_id: String,
}

#[cfg(test)]
mod tests {
    use super::parse_not_ready_reasons;

    #[test]
    fn not_ready_projection_names_every_exact_scenario_reason() {
        let bytes = br#"{
            "verdict": {
                "status": "not-ready",
                "reasons": [
                    {"kind": "scenario-unsupported", "scenario_id": "sandbox-execution"},
                    {"kind": "scenario-failed", "scenario_id": "tui-lifecycle"}
                ]
            }
        }"#;
        assert_eq!(
            parse_not_ready_reasons(bytes).expect("valid NotReady report"),
            "scenario-unsupported:sandbox-execution, scenario-failed:tui-lifecycle"
        );
    }

    #[test]
    fn failed_process_cannot_hide_behind_a_ready_report() {
        let error = parse_not_ready_reasons(br#"{"verdict":{"status":"ready"}}"#)
            .expect_err("Ready must contradict a failed process");
        assert!(error.to_string().contains("claimed Ready"));
    }
}
