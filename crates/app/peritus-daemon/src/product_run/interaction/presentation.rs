//! Human-readable projections of completed pipeline response envelopes.
//!
//! The stored activity and private trace remain exact. This projection grants no acceptance;
//! the runner's typed settlement remains the authority for success.

use super::{ProductActivity, ProductActivityKind, bounded};
use serde::Deserialize;
use std::fmt::Write as _;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    kind: CompletionKind,
    summary: String,
    run_instructions: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CompletionKind {
    Complete,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    summary: String,
    findings: Vec<Finding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Finding {
    category: String,
    severity: String,
    title: String,
    description: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    reproduction: String,
    remediation: String,
}

pub(super) fn pipeline_activity(activity: &ProductActivity) -> ProductActivity {
    if activity.kind() != ProductActivityKind::Assistant {
        return activity.clone();
    }
    let rendered = if let Ok(completion) = serde_json::from_str::<Completion>(activity.text()) {
        let CompletionKind::Complete = completion.kind;
        if completion.run_instructions.trim().is_empty() {
            completion.summary
        } else {
            format!("{}\n\nRun: {}", completion.summary, completion.run_instructions)
        }
    } else if let Ok(review) = serde_json::from_str::<Review>(activity.text()) {
        let mut text = review.summary;
        for finding in review.findings {
            let _ = write!(
                text,
                "\n\n{}: {} [{}]\n{}\nLocation: {}\nReproduce: {}\nSuggested fix: {}",
                finding.severity,
                finding.title,
                finding.category,
                finding.description,
                finding.location,
                finding.reproduction,
                finding.remediation
            );
        }
        text
    } else {
        return activity.clone();
    };
    ProductActivity::new(
        activity.sequence(),
        activity.kind(),
        bounded(&rendered),
        activity.detail().to_owned(),
    )
    .unwrap_or_else(|_| activity.clone())
}
