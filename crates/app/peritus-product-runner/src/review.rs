//! Typed independent-review parsing and conserved finding rendering.

use peritus_review::{
    FindingSeverity, ProductFinding, ProductFindingCategory, ProductFindingLedger,
    ProductFindingState, ProductReviewSubmission,
};
use serde::Deserialize;
use serde::Serialize;

use crate::model_output::{TypedObjectError, last_typed_object};
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
    let review: ReviewWire = match last_typed_object(value) {
        Ok(review) => review,
        Err(TypedObjectError::Missing) => {
            return Err(invalid("review contains no JSON object"));
        }
        Err(TypedObjectError::Invalid(detail)) => {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "parse typed reviewer result",
                detail,
            ));
        }
    };
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
