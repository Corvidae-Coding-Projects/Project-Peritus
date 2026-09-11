//! Preserve reachability and every tool outcome; compilation failure is not a caught bug.

use super::{runner, write_json};
use crate::error::XtaskError;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const SLICES: [(&str, &str, Option<&str>); 3] = [
    ("peritus-context", "crates/orchestration/peritus-context/src/working/selection.rs", None),
    (
        "peritus-product-runner",
        "crates/app/peritus-product-runner/src/developer_tools/receipt.rs",
        Some("EffectReceiptLedger::(begin|complete|load)"),
    ),
    (
        "peritus-daemon",
        "crates/app/peritus-daemon/src/product_run/lifecycle.rs",
        Some("ProductRunService::(shutdown|resume_interrupted|cancel|retry)"),
    ),
];

pub(super) fn run(
    root: &Path,
    evidence: &Path,
    index: usize,
    shard: Option<usize>,
) -> Result<(), XtaskError> {
    let mut inventory = command(index)?;
    inventory.args(["--list", "--json"]);
    runner::checked(root, evidence, "inventory", inventory, 30)?;
    let bytes = fs::read(evidence.join("inventory.stdout"))
        .map_err(|error| XtaskError::io("read mutant inventory", evidence, error))?;
    let inventory: Value = serde_json::from_slice(&bytes).map_err(XtaskError::metadata_decode)?;
    let discovered = inventory
        .as_array()
        .ok_or_else(|| XtaskError::metadata("mutant inventory must be an array"))?
        .len();
    if discovered == 0 {
        write_json(
            &evidence.join("mutation-summary.json"),
            &json!({"status": "unreachable", "discovered": 0, "baseline": "not_run", "reason": "no mutations discovered; inspect macro/generated code and run a curated behavioral canary"}),
        )?;
        return Err(XtaskError::metadata(
            "mutation slice is unreachable; zero discovered mutations cannot pass",
        ));
    }
    let selected = if let Some(shard) = shard {
        let mut listing = command(index)?;
        listing.args(["--list", "--json", "--shard", &format!("{shard}/8")]);
        runner::checked(root, evidence, "selected-inventory", listing, 30)?;
        let bytes = fs::read(evidence.join("selected-inventory.stdout"))
            .map_err(|error| XtaskError::io("read selected inventory", evidence, error))?;
        serde_json::from_slice(&bytes).map_err(XtaskError::metadata_decode)?
    } else {
        inventory
    };
    let selected_count = selected
        .as_array()
        .ok_or_else(|| XtaskError::metadata("selected inventory must be an array"))?
        .len();
    let outside = discovered.checked_sub(selected_count).ok_or_else(|| {
        XtaskError::metadata("selected mutation inventory exceeds full inventory")
    })?;
    write_json(
        &evidence.join("selection.json"),
        &json!({"discovered": discovered, "selected": selected_count, "outside_this_shard": outside, "shard": shard, "shard_count": if shard.is_some() { 8 } else { 1 }}),
    )?;
    if selected_count == 0 {
        return Err(XtaskError::metadata("empty mutation shard; no executed campaign"));
    }
    let mut campaign = command(index)?;
    if let Some(shard) = shard {
        campaign.args(["--shard", &format!("{shard}/8")]);
    }
    campaign.args([
        "--baseline",
        "run",
        "--no-shuffle",
        "--jobs",
        "1",
        "--build-timeout",
        "180",
        "--timeout",
        "60",
        "--output",
    ]);
    campaign.arg(evidence);
    let outcome = runner::run(root, evidence, "mutation", campaign, Duration::from_mins(8))?;
    let path = evidence.join("mutants.out/outcomes.json");
    let outcomes =
        fs::read(&path).map_err(|error| XtaskError::io("read mutation outcomes", &path, error))?;
    let outcomes: Value = serde_json::from_slice(&outcomes).map_err(XtaskError::metadata_decode)?;
    let summary = summarize(&selected, &outcomes)?;
    let complete = summary["status"] == "completed";
    write_json(&evidence.join("mutation-summary.json"), &summary)?;
    if !complete || outcome.timed_out || !outcome.status.success() {
        return Err(XtaskError::metadata(
            "mutation campaign failed or incomplete; retain baseline, caught, missed, unviable, timeout and untested counts separately",
        ));
    }
    Ok(())
}

fn command(index: usize) -> Result<Command, XtaskError> {
    let (package, file, regex) =
        SLICES.get(index).ok_or_else(|| XtaskError::invocation("unknown mutation slice"))?;
    let mut command = Command::new("cargo");
    command.args([
        "mutants",
        "--no-config",
        "--package",
        package,
        "--file",
        file,
        "--test-package",
        package,
        "--cap-lints",
        "false",
        "--cargo-arg=--locked",
        "--cargo-arg=--offline",
    ]);
    if let Some(regex) = regex {
        command.args(["--re", regex]);
    }
    Ok(command)
}

pub(super) fn summarize(inventory: &Value, report: &Value) -> Result<Value, XtaskError> {
    let inventory =
        inventory.as_array().ok_or_else(|| XtaskError::metadata("mutation inventory missing"))?;
    let inventory: Vec<_> = inventory
        .iter()
        .cloned()
        .map(|mut item| {
            if let Some(object) = item.as_object_mut() {
                object.remove("diff");
            }
            item
        })
        .collect();
    let outcomes = report["outcomes"]
        .as_array()
        .ok_or_else(|| XtaskError::metadata("mutation outcomes missing"))?;
    let mut baseline = "not_run";
    let mut seen = Vec::new();
    let (mut caught, mut missed, mut unviable, mut timeout, mut other) = (0, 0, 0, 0, 0);
    for outcome in outcomes {
        let summary = outcome["summary"].as_str().unwrap_or("unknown");
        if outcome["scenario"] == "Baseline" {
            if baseline != "not_run" {
                return Err(XtaskError::metadata("duplicate mutation baseline"));
            }
            baseline = if summary == "Success" { "passed" } else { "failed" };
            continue;
        }
        let mutant = &outcome["scenario"]["Mutant"];
        if mutant.is_null() || !inventory.contains(mutant) || seen.contains(&mutant) {
            return Err(XtaskError::metadata("unknown or duplicate mutation outcome"));
        }
        seen.push(mutant);
        match summary {
            "CaughtMutant" => caught += 1,
            "MissedMutant" => missed += 1,
            "Unviable" => unviable += 1,
            "Timeout" => timeout += 1,
            _ => other += 1,
        }
    }
    let untested = inventory.len() - seen.len();
    let complete = !inventory.is_empty()
        && baseline == "passed"
        && untested == 0
        && other == 0
        && timeout == 0
        && missed == 0;
    Ok(json!({
        "status": if complete { "completed" } else { "failed_or_incomplete" }, "baseline": baseline,
        "discovered": inventory.len(), "caught": caught, "missed": missed, "unviable": unviable,
        "timeout": timeout, "untested": untested, "unclassified": other,
        "equivalent": "requires independent review; never inferred from survival",
        "behavioral_detection_demonstrated": caught > 0,
    }))
}
