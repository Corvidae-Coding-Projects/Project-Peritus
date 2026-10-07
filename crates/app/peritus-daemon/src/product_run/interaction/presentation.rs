//! Human-readable projections of completed pipeline response envelopes.
//!
//! The stored activity and private trace remain exact. This projection grants no acceptance;
//! the runner's typed settlement remains the authority for success.

use super::{ProductActivity, ProductActivityKind};
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
        rendered,
        activity.detail().to_owned(),
    )
    .unwrap_or_else(|_| activity.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(text: &str) -> ProductActivity {
        ProductActivity::new(1, ProductActivityKind::Assistant, text.to_owned(), String::new())
            .unwrap()
    }

    #[test]
    fn pipeline_envelopes_show_delivered_text_and_preserve_every_finding_field() {
        let complete = activity(
            r#"{"kind":"complete","summary":"Added version 1.1.","run_instructions":"python tasks.py --version"}"#,
        );
        let projected = pipeline_activity(&complete);
        assert_eq!(projected.text(), "Added version 1.1.\n\nRun: python tasks.py --version");
        assert!(complete.text().starts_with('{'), "original remains exact");
        let review = activity(
            r#"{"summary":"Changes needed.","findings":[{"category":"correctness","severity":"blocking","title":"Empty input","description":"Crashes.","location":"tasks.py:7","reproduction":"Run with no args.","remediation":"Validate arguments."}]}"#,
        );
        let projected = pipeline_activity(&review);
        for value in [
            "Changes needed.",
            "correctness",
            "blocking",
            "Empty input",
            "Crashes.",
            "tasks.py:7",
            "Run with no args.",
            "Validate arguments.",
        ] {
            assert!(projected.text().contains(value));
        }
        let ordinary = activity(r#"{"summary":"user data","unrelated":true}"#);
        assert_eq!(pipeline_activity(&ordinary), ordinary);
        let partial = activity(r#"{"kind":"complete","summary":"not finished"#);
        assert_eq!(pipeline_activity(&partial), partial);
    }
}
