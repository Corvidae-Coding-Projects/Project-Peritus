//! Typed independent-review parsing and conserved finding rendering.

use peritus_review::{
    FindingSeverity, ProductFinding, ProductFindingCategory, ProductFindingLedger,
    ProductFindingState, ProductReviewSubmission,
};
use serde::Deserialize;
use serde::Serialize;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewWire {
    summary: String,
    findings: Vec<FindingWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindingWire {
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

pub fn parse(value: &str, cycle: u32) -> Result<ProductReviewSubmission, ProductRunnerError> {
    let start = value.find('{').ok_or_else(|| invalid("review contains no JSON object"))?;
    let end = value.rfind('}').ok_or_else(|| invalid("review contains no complete JSON object"))?;
    let review: ReviewWire = serde_json::from_str(&value[start..=end]).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "parse typed reviewer result",
            error.to_string(),
        )
    })?;
    let findings = review
        .findings
        .into_iter()
        .map(|finding| {
            let category = ProductFindingCategory::parse(&finding.category)
                .ok_or_else(|| invalid("review finding category is unknown"))?;
            let severity = severity(&finding.severity)
                .ok_or_else(|| invalid("review finding severity is unknown"))?;
            ProductFinding::new(
                category,
                severity,
                finding.title,
                finding.description,
                finding.location,
                finding.reproduction,
                finding.remediation,
                cycle,
            )
            .map_err(|error| invalid_owned(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ProductReviewSubmission::new(review.summary, findings)
        .map_err(|error| invalid_owned(error.to_string()))
}

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn render(ledger: &ProductFindingLedger) -> String {
    let mut text = format!("Review cycle {}: {}\n", ledger.cycle(), ledger.review_summary());
    let findings = ledger.findings().collect::<Vec<_>>();
    if findings.is_empty() {
        text.push_str("No findings.\n");
        return text;
    }
    for finding in findings {
        let state = match finding.state() {
            ProductFindingState::Open => "open".to_owned(),
            ProductFindingState::FixProposed { cycle } => {
                format!("fix proposed in cycle {cycle}; reviewer confirmation pending")
            }
            ProductFindingState::ResolutionConfirmed { cycle } => {
                format!("resolution confirmed in cycle {cycle}")
            }
        };
        text.push_str(&format!(
            "\n[{} / {:?} / {}]\n{}\n{}\nLocation: {}\nReproduce: {}\nRemediation: {}\n",
            finding.category().as_str(),
            finding.severity(),
            state,
            finding.title(),
            finding.description(),
            finding.location(),
            finding.reproduction(),
            finding.remediation(),
        ));
    }
    text
}

pub fn encode_ledger(ledger: &ProductFindingLedger) -> Result<String, ProductRunnerError> {
    let findings = ledger
        .findings()
        .map(|finding| {
            let (state, state_cycle) = match finding.state() {
                ProductFindingState::Open => ("open", 0),
                ProductFindingState::FixProposed { cycle } => ("fix_proposed", cycle),
                ProductFindingState::ResolutionConfirmed { cycle } => {
                    ("resolution_confirmed", cycle)
                }
            };
            DurableFindingWire {
                category: finding.category().as_str().to_owned(),
                severity: severity_text(finding.severity()).to_owned(),
                title: finding.title().to_owned(),
                description: finding.description().to_owned(),
                location: finding.location().to_owned(),
                reproduction: finding.reproduction().to_owned(),
                remediation: finding.remediation().to_owned(),
                state: state.to_owned(),
                state_cycle,
                first_cycle: finding.first_cycle(),
                last_cycle: finding.last_cycle(),
            }
        })
        .collect();
    serde_json::to_string(&DurableLedgerWire {
        cycle: ledger.cycle(),
        summary: ledger.review_summary().to_owned(),
        findings,
    })
    .map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "encode durable D2 finding ledger",
            error.to_string(),
        )
    })
}

pub fn restore_ledger(value: &str) -> Result<ProductFindingLedger, ProductRunnerError> {
    if value.is_empty() {
        return Ok(ProductFindingLedger::new());
    }
    let wire: DurableLedgerWire = serde_json::from_str(value).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "decode durable D2 finding ledger",
            error.to_string(),
        )
    })?;
    let findings = wire
        .findings
        .into_iter()
        .map(|finding| {
            let category = ProductFindingCategory::parse(&finding.category)
                .ok_or_else(|| invalid("durable finding category is unknown"))?;
            let severity = severity(&finding.severity)
                .ok_or_else(|| invalid("durable finding severity is unknown"))?;
            let state = match finding.state.as_str() {
                "open" if finding.state_cycle == 0 => ProductFindingState::Open,
                "fix_proposed" => ProductFindingState::FixProposed { cycle: finding.state_cycle },
                "resolution_confirmed" => {
                    ProductFindingState::ResolutionConfirmed { cycle: finding.state_cycle }
                }
                _ => return Err(invalid("durable finding state is invalid")),
            };
            ProductFinding::restore(
                category,
                severity,
                finding.title,
                finding.description,
                finding.location,
                finding.reproduction,
                finding.remediation,
                state,
                finding.first_cycle,
                finding.last_cycle,
            )
            .map_err(|error| invalid_owned(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ProductFindingLedger::restore(wire.cycle, wire.summary, findings)
        .map_err(|error| invalid_owned(error.to_string()))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableLedgerWire {
    cycle: u32,
    summary: String,
    findings: Vec<DurableFindingWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingWire {
    category: String,
    severity: String,
    title: String,
    description: String,
    location: String,
    reproduction: String,
    remediation: String,
    state: String,
    state_cycle: u32,
    first_cycle: u32,
    last_cycle: u32,
}

fn severity(value: &str) -> Option<FindingSeverity> {
    match value {
        "advisory" => Some(FindingSeverity::Advisory),
        "low" => Some(FindingSeverity::Low),
        "medium" => Some(FindingSeverity::Medium),
        "high" => Some(FindingSeverity::High),
        "critical" => Some(FindingSeverity::Critical),
        _ => None,
    }
}

const fn severity_text(value: FindingSeverity) -> &'static str {
    match value {
        FindingSeverity::Advisory => "advisory",
        FindingSeverity::Low => "low",
        FindingSeverity::Medium => "medium",
        FindingSeverity::High => "high",
        FindingSeverity::Critical => "critical",
    }
}

fn invalid(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate typed reviewer result",
        detail,
    )
}

fn invalid_owned(detail: String) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate typed reviewer result",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewer_boolean_is_rejected_and_advisory_remains_nonblocking() {
        assert!(parse(r#"{"summary":"looks fine","blocking":false,"findings":[]}"#, 1).is_err());
        assert!(parse(r#"{"summary":"still inspecting"}"#, 1).is_err());
        let submission = parse(
            r#"{"summary":"target was missed","findings":[{"category":"build_coverage","severity":"advisory","title":"Nested target not built","description":"Only root tests ran","location":"game/Cargo.toml","reproduction":"cargo check --manifest-path game/Cargo.toml","remediation":"Run exact target gates"}]}"#,
            1,
        )
        .expect("typed review");
        let mut ledger = ProductFindingLedger::new();
        ledger.admit_review(1, submission).expect("admit");
        assert!(!ledger.has_blockers());

        let submission = parse(
            r#"{"summary":"target remains uncovered","findings":[{"category":"build_coverage","severity":"low","title":"Nested target not built","description":"Only root tests ran","location":"game/Cargo.toml","reproduction":"cargo check --manifest-path game/Cargo.toml","remediation":"Run exact target gates"}]}"#,
            2,
        )
        .expect("typed review");
        ledger.admit_review(2, submission).expect("admit");
        assert!(ledger.has_blockers());
    }

    #[test]
    fn durable_ledger_round_trip_preserves_a_fixer_proposal() {
        let submission = parse(
            r#"{"summary":"target was missed","findings":[{"category":"build_coverage","severity":"high","title":"Nested target not built","description":"Only root tests ran","location":"game/Cargo.toml","reproduction":"cargo check --manifest-path game/Cargo.toml","remediation":"Run exact target gates"}]}"#,
            1,
        )
        .expect("typed review");
        let mut ledger = ProductFindingLedger::new();
        ledger.admit_review(1, submission).expect("admit");
        ledger.record_fixer_proposal(1);
        let restored = restore_ledger(&encode_ledger(&ledger).expect("encode")).expect("restore");
        assert!(matches!(
            restored.open_findings().next().expect("finding").state(),
            ProductFindingState::FixProposed { cycle: 1 },
        ));
    }
}
