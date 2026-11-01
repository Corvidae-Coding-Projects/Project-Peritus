//! Product finding conservation against an independent logical-key ledger.
use peritus_review::{
    FindingSeverity, ProductFinding, ProductFindingCategory, ProductFindingLedger,
    ProductFindingState, ProductReviewSubmission,
};
use peritus_types::Sha256Digest;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    History { operations: Vec<Operation> },
    ExhaustedCycle,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub category: u8,
    pub severity: u8,
    pub key: u8,
    pub variant: u8,
    pub detail: u8,
    pub location: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Review { findings: Vec<Finding> },
    Fix,
    Restore { duplicate: bool, reverse: bool },
    RejectCycle,
    RejectFindingCycle,
    RejectDuplicate { finding: Finding },
    RejectRestore,
    Read,
}

fn finding_strategy() -> BoxedStrategy<Finding> {
    (0_u8..7, 0_u8..5, 0_u8..8, 0_u8..3, any::<u8>(), any::<u8>())
        .prop_map(|(category, severity, key, variant, detail, location)| Finding {
            category,
            severity,
            key,
            variant,
            detail,
            location,
        })
        .boxed()
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        8=>prop::collection::vec(finding_strategy(),0..=12).prop_map(|values|{let unique:BTreeMap<_,_>=values.into_iter().map(|finding|((finding.category,finding.key),finding)).collect();Operation::Review{findings:unique.into_values().collect()}}),
        4=>Just(Operation::Fix),
        3=>(any::<bool>(),any::<bool>()).prop_map(|(duplicate,reverse)|Operation::Restore{duplicate,reverse}),
        1=>Just(Operation::RejectCycle),1=>Just(Operation::RejectFindingCycle),
        1=>finding_strategy().prop_map(|finding|Operation::RejectDuplicate{finding}),
        1=>Just(Operation::RejectRestore),1=>Just(Operation::Read),
    ];
    prop_oneof![20=>prop::collection::vec(operation,0..=max_steps.min(256)).prop_map(|operations|Case::History{operations}),1=>Just(Case::ExhaustedCycle)].boxed()
}
fn category(value: u8) -> ProductFindingCategory {
    match value {
        0 => ProductFindingCategory::Correctness,
        1 => ProductFindingCategory::RequestedBehavior,
        2 => ProductFindingCategory::BuildCoverage,
        3 => ProductFindingCategory::TestCoverage,
        4 => ProductFindingCategory::Security,
        5 => ProductFindingCategory::Maintainability,
        _ => ProductFindingCategory::Documentation,
    }
}
fn category_name(value: u8) -> &'static str {
    match value {
        0 => "correctness",
        1 => "requested_behavior",
        2 => "build_coverage",
        3 => "test_coverage",
        4 => "security",
        5 => "maintainability",
        _ => "documentation",
    }
}
fn severity(value: u8) -> FindingSeverity {
    match value {
        0 => FindingSeverity::Advisory,
        1 => FindingSeverity::Low,
        2 => FindingSeverity::Medium,
        3 => FindingSeverity::High,
        _ => FindingSeverity::Critical,
    }
}
fn canonical_title(key: u8) -> &'static str {
    match key {
        0 => "finding zero",
        1 => "memory stale",
        2 => "cache race",
        3 => "café",
        4 => "i\u{307}stanbul",
        5 => "straße",
        6 => "role drift",
        _ => "file mismatch",
    }
}
fn title(finding: &Finding) -> String {
    match finding.variant {
        0 => canonical_title(finding.key).into(),
        1 => match finding.key {
            4 => " İSTANBUL ".into(),
            5 => " STRAẞE ".into(),
            _ => format!(" {} ", canonical_title(finding.key).to_uppercase()),
        },
        _ => format!("\t{}\n", canonical_title(finding.key)),
    }
}
fn identity(key: (u8, u8)) -> Sha256Digest {
    let mut bytes = b"peritus.product-finding.v2\0".to_vec();
    bytes.extend_from_slice(category_name(key.0).as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(canonical_title(key.1).as_bytes());
    peritus_codec::sha256(&bytes)
}
fn production_finding(finding: &Finding, cycle: u32) -> Result<ProductFinding, String> {
    ProductFinding::new(
        category(finding.category),
        severity(finding.severity),
        title(finding),
        format!("description-{}", finding.detail),
        format!("source-{}:7", finding.location),
        format!("reproduce-{}", finding.detail),
        format!("remediate-{}", finding.detail),
        cycle,
    )
    .map_err(|error| error.to_string())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Open,
    Proposed(u32),
    Resolved(u32),
}
#[derive(Clone)]
struct Entry {
    finding: Finding,
    title: String,
    first: u32,
    last: u32,
    state: State,
}
#[derive(Default)]
struct Model {
    cycle: u32,
    fixes: u32,
    summary: String,
    entries: BTreeMap<(u8, u8), Entry>,
}
fn bump(metrics: &mut crate::Metrics, key: &str, value: u64) {
    *metrics.entry(key.into()).or_default() += value;
}
fn blocked(entry: &Entry) -> bool {
    entry.finding.severity != 0 && (entry.finding.category <= 4 || entry.finding.severity >= 3)
}
fn state(value: State) -> ProductFindingState {
    match value {
        State::Open => ProductFindingState::Open,
        State::Proposed(cycle) => ProductFindingState::FixProposed { cycle },
        State::Resolved(cycle) => ProductFindingState::ResolutionConfirmed { cycle },
    }
}
fn restored(entry: &Entry) -> Result<ProductFinding, String> {
    ProductFinding::restore(
        category(entry.finding.category),
        severity(entry.finding.severity),
        entry.title.clone(),
        format!("description-{}", entry.finding.detail),
        format!("source-{}:7", entry.finding.location),
        format!("reproduce-{}", entry.finding.detail),
        format!("remediate-{}", entry.finding.detail),
        state(entry.state),
        entry.first,
        entry.last,
    )
    .map_err(|error| error.to_string())
}

fn compare(
    model: &Model,
    ledger: &ProductFindingLedger,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    if ledger.cycle() != model.cycle
        || ledger.review_summary() != model.summary
        || ledger.findings().count() != model.entries.len()
    {
        return Err("review head/summary/history count disagrees with logical ledger".into());
    }
    let expected_open =
        model.entries.values().filter(|entry| !matches!(entry.state, State::Resolved(_))).count();
    if ledger.open_findings().count() != expected_open
        || ledger.has_blockers()
            != model
                .entries
                .values()
                .any(|entry| !matches!(entry.state, State::Resolved(_)) && blocked(entry))
    {
        return Err("finding blockers or unresolved projection disagrees with policy table".into());
    }
    for (key, entry) in &model.entries {
        let actual = ledger
            .findings()
            .find(|finding| finding.id() == identity(*key))
            .ok_or_else(|| format!("logical finding {key:?} lost stable identity"))?;
        if actual.category() != category(key.0)
            || actual.severity() != severity(entry.finding.severity)
            || actual.title() != entry.title
            || actual.description() != format!("description-{}", entry.finding.detail)
            || actual.location() != format!("source-{}:7", entry.finding.location)
            || actual.reproduction() != format!("reproduce-{}", entry.finding.detail)
            || actual.remediation() != format!("remediate-{}", entry.finding.detail)
            || actual.first_cycle() != entry.first
            || actual.last_cycle() != entry.last
            || actual.state() != state(entry.state)
            || actual.blocking() != blocked(entry)
        {
            return Err(format!(
                "finding {key:?} content/cycles/state diverged: {actual:?}, expected {:?}",
                entry.state
            ));
        }
    }
    bump(metrics, "ledger_comparisons", 1);
    Ok(())
}
fn admit(
    model: &mut Model,
    findings: &[Finding],
    metrics: &mut crate::Metrics,
) -> Result<ProductReviewSubmission, String> {
    let cycle = model.cycle.checked_add(1).ok_or("review cycle exhausted")?;
    let mut submitted = BTreeMap::new();
    for finding in findings {
        if finding.category >= 7
            || finding.key >= 8
            || finding.variant >= 3
            || finding.severity >= 5
        {
            return Err("finding exceeds logical input range".into());
        }
        if submitted.insert((finding.category, finding.key), finding).is_some() {
            return Err("positive history contains duplicate logical key".into());
        }
    }
    for (key, entry) in &mut model.entries {
        if !submitted.contains_key(key) {
            match entry.state {
                State::Proposed(_) => {
                    entry.state = State::Resolved(cycle);
                    entry.last = cycle;
                    bump(metrics, "confirmed_after_fixer", 1);
                }
                State::Open => bump(metrics, "untouched_omissions_conserved", 1),
                State::Resolved(_) => {}
            }
        }
    }
    for (key, finding) in submitted {
        if let Some(entry) = model.entries.get_mut(&key) {
            if matches!(entry.state, State::Resolved(_)) {
                bump(metrics, "resolved_findings_reopened", 1);
            }
            if entry.finding.location != finding.location {
                bump(metrics, "relocated_findings", 1);
            }
            entry.finding = finding.clone();
            entry.last = cycle;
            entry.state = State::Open;
            bump(metrics, "reobserved_findings", 1);
        } else {
            model.entries.insert(
                key,
                Entry {
                    finding: finding.clone(),
                    title: title(finding),
                    first: cycle,
                    last: cycle,
                    state: State::Open,
                },
            );
            bump(metrics, "new_findings", 1);
        }
    }
    model.cycle = cycle;
    model.summary = format!("review-cycle-{cycle}");
    ProductReviewSubmission::new(
        model.summary.clone(),
        findings
            .iter()
            .map(|finding| production_finding(finding, cycle))
            .collect::<Result<_, _>>()?,
    )
    .map_err(|error| error.to_string())
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    let mut metrics = crate::Metrics::new();
    let Case::History { operations } = case else {
        let mut ledger =
            ProductFindingLedger::restore(u32::MAX, "exhausted head".into(), Vec::new())
                .map_err(|error| error.to_string())?;
        let before = ledger.clone();
        let submission = ProductReviewSubmission::new("same cycle must reject".into(), Vec::new())
            .map_err(|error| error.to_string())?;
        if ledger.admit_review(u32::MAX, submission).is_ok() {
            return Err(
                "non-advancing review at u32::MAX was accepted and replaced the conserved head"
                    .into(),
            );
        }
        if ledger != before {
            return Err("exhausted-cycle rejection mutated ledger".into());
        }
        bump(&mut metrics, "exhausted_cycle_rejections", 1);
        bump(&mut metrics, "cases", 1);
        return Ok(metrics);
    };
    if operations.len() > 512 {
        return Err("review operation bound exceeded".into());
    }
    let mut ledger = ProductFindingLedger::new();
    let mut model = Model::default();
    compare(&model, &ledger, &mut metrics)?;
    for (step, operation) in operations.iter().enumerate() {
        let result = (|| {
            match operation {
                Operation::Review { findings } => {
                    let submission = admit(&mut model, findings, &mut metrics)?;
                    ledger
                        .admit_review(model.cycle, submission)
                        .map_err(|error| error.to_string())?;
                    bump(&mut metrics, "accepted_reviews", 1);
                }
                Operation::Fix => {
                    model.fixes = model.fixes.checked_add(1).ok_or("fixer cycle exhausted")?;
                    for entry in model
                        .entries
                        .values_mut()
                        .filter(|entry| !matches!(entry.state, State::Resolved(_)))
                    {
                        entry.state = State::Proposed(model.fixes);
                        bump(&mut metrics, "fix_proposals_pending_review", 1);
                    }
                    ledger.record_fixer_proposal(model.fixes);
                    bump(&mut metrics, "fixer_cycles", 1);
                }
                Operation::Restore { duplicate, reverse } => {
                    let mut findings =
                        model.entries.values().map(restored).collect::<Result<Vec<_>, _>>()?;
                    if *duplicate {
                        findings.extend(findings.clone());
                        bump(
                            &mut metrics,
                            "duplicate_restore_rows",
                            u64::try_from(model.entries.len())
                                .map_err(|error| error.to_string())?,
                        );
                    }
                    if *reverse {
                        findings.reverse();
                    }
                    let reconstructed =
                        ProductFindingLedger::restore(model.cycle, model.summary.clone(), findings)
                            .map_err(|error| error.to_string())?;
                    if reconstructed != ledger {
                        return Err("public ledger reconstruction changed exact history".into());
                    }
                    ledger = reconstructed;
                    bump(&mut metrics, "ledger_reconstructions", 1);
                }
                Operation::RejectCycle | Operation::RejectFindingCycle => {
                    let before = ledger.clone();
                    let next = model.cycle.checked_add(1).ok_or("review cycle exhausted")?;
                    let finding_cycle = if matches!(operation, Operation::RejectFindingCycle) {
                        next.checked_add(1).ok_or("review cycle exhausted")?
                    } else {
                        next
                    };
                    let finding = Finding {
                        category: 0,
                        severity: 3,
                        key: 0,
                        variant: 0,
                        detail: 0,
                        location: 0,
                    };
                    let submission = ProductReviewSubmission::new(
                        "invalid cycle".into(),
                        vec![production_finding(&finding, finding_cycle)?],
                    )
                    .map_err(|error| error.to_string())?;
                    let requested = if matches!(operation, Operation::RejectCycle) {
                        model.cycle
                    } else {
                        next
                    };
                    if ledger.admit_review(requested, submission).is_ok() || ledger != before {
                        return Err("invalid reviewer cycle accepted or partially applied".into());
                    }
                    bump(&mut metrics, "cycle_rejections", 1);
                }
                Operation::RejectDuplicate { finding } => {
                    let cycle = model.cycle.checked_add(1).ok_or("review cycle exhausted")?;
                    let mut alias = finding.clone();
                    alias.variant = (alias.variant + 1) % 3;
                    alias.location = alias.location.wrapping_add(1);
                    if ProductReviewSubmission::new(
                        "duplicate alias".into(),
                        vec![
                            production_finding(finding, cycle)?,
                            production_finding(&alias, cycle)?,
                        ],
                    )
                    .is_ok()
                    {
                        return Err("duplicate normalized title/category was accepted".into());
                    }
                    bump(&mut metrics, "duplicate_submission_rejections", 1);
                }
                Operation::RejectRestore => {
                    let finding = Finding {
                        category: 0,
                        severity: 3,
                        key: 0,
                        variant: 0,
                        detail: 0,
                        location: 0,
                    };
                    let actual = production_finding(&finding, 2)?;
                    if ProductFindingLedger::restore(1, "too old".into(), vec![actual]).is_ok() {
                        return Err("restore accepted a finding newer than its ledger head".into());
                    }
                    bump(&mut metrics, "restore_rejections", 1);
                }
                Operation::Read => bump(&mut metrics, "explicit_reads", 1),
            }
            compare(&model, &ledger, &mut metrics)
        })();
        result.map_err(|error| format!("step {step} {operation:?}: {error}"))?;
        bump(&mut metrics, "operations", 1);
    }
    bump(&mut metrics, "cases", 1);
    Ok(metrics)
}

pub fn calibration_case() -> Case {
    Case::History {
        operations: vec![
            Operation::Review {
                findings: vec![Finding {
                    category: 0,
                    severity: 3,
                    key: 0,
                    variant: 0,
                    detail: 1,
                    location: 1,
                }],
            },
            Operation::Review { findings: Vec::new() },
        ],
    }
}
pub fn examples() -> Vec<Case> {
    let mut broad = match calibration_case() {
        Case::History { operations } => operations,
        Case::ExhaustedCycle => Vec::new(),
    };
    broad.extend([
        Operation::Fix,
        Operation::Read,
        Operation::Restore { duplicate: true, reverse: true },
        Operation::Review { findings: Vec::new() },
        Operation::Review {
            findings: vec![Finding {
                category: 0,
                severity: 4,
                key: 0,
                variant: 1,
                detail: 2,
                location: 9,
            }],
        },
        Operation::RejectCycle,
        Operation::RejectFindingCycle,
        Operation::RejectRestore,
    ]);
    let policy = Case::History {
        operations: (0_u8..7)
            .flat_map(|category| {
                (0_u8..5).map(move |severity| Operation::Review {
                    findings: vec![Finding {
                        category,
                        severity,
                        key: category,
                        variant: severity % 3,
                        detail: severity,
                        location: severity,
                    }],
                })
            })
            .collect(),
    };
    let aliases = Case::History {
        operations: (0_u8..8)
            .flat_map(|key| {
                [
                    Operation::Review {
                        findings: vec![Finding {
                            category: 4,
                            severity: 2,
                            key,
                            variant: 0,
                            detail: 1,
                            location: 1,
                        }],
                    },
                    Operation::RejectDuplicate {
                        finding: Finding {
                            category: 4,
                            severity: 2,
                            key,
                            variant: 1,
                            detail: 2,
                            location: 2,
                        },
                    },
                    Operation::Review {
                        findings: vec![Finding {
                            category: 4,
                            severity: 3,
                            key,
                            variant: 1,
                            detail: 3,
                            location: 3,
                        }],
                    },
                ]
            })
            .collect(),
    };
    vec![
        calibration_case(),
        Case::History { operations: broad },
        policy,
        aliases,
        Case::ExhaustedCycle,
    ]
}
