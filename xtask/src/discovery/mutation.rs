//! Preserve reachability and every tool outcome; compilation failure is not a caught bug.

use super::{runner, write_json};
use crate::error::XtaskError;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use self::repository::MutationRepository;

mod environment;
mod repository;

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

const CONTEXT_SOURCE: &str = "crates/orchestration/peritus-context/src/working/selection.rs";
const CONTEXT_TEST: &str =
    "headroom_preserves_required_roots_and_shared_dependencies_without_optional_expansion";
const CONTEXT_CHECK: &str =
    "if needed <= allocation || needed > available_tokens { return Err(WorkingError::Capacity); }";
const CONTEXT_MUTANT: &str = "if needed <= allocation { return Err(WorkingError::Capacity); }";

pub(super) fn context_canary(root: &Path, evidence: &Path) -> Result<(), XtaskError> {
    require_clean_tracked_source(root)?;
    let repository = evidence.join("context-canary-repository");
    let result = context_canary_inner(root, evidence, &repository);
    let cleanup = if repository.exists() {
        fs::remove_dir_all(&repository)
            .map_err(|error| XtaskError::io("remove owned canary repository", &repository, error))
    } else {
        Ok(())
    };
    result?;
    cleanup
}

fn context_canary_inner(root: &Path, evidence: &Path, repository: &Path) -> Result<(), XtaskError> {
    let mut clone = Command::new("git");
    clone.args(["clone", "--quiet", "--no-local", "--no-hardlinks"]).arg(root).arg(repository);
    runner::checked(root, evidence, "context-canary-clone", clone, 60)?;

    runner::checked(repository, evidence, "context-canary-baseline", context_test_command(), 180)?;
    let source = repository.join(CONTEXT_SOURCE);
    let original = fs::read_to_string(&source)
        .map_err(|error| XtaskError::io("read context canary source", &source, error))?;
    if original.match_indices(CONTEXT_CHECK).count() != 1 {
        return Err(XtaskError::metadata(
            "context canary source did not contain exactly one reviewed capacity check",
        ));
    }
    fs::write(&source, original.replace(CONTEXT_CHECK, CONTEXT_MUTANT))
        .map_err(|error| XtaskError::io("write context canary mutant", &source, error))?;
    let diff = git_output(repository, &["diff", "--binary", "--", CONTEXT_SOURCE])?;
    fs::write(evidence.join("context-canary.patch"), diff)
        .map_err(|error| XtaskError::io("write context canary patch", evidence, error))?;

    let outcome = runner::run(
        repository,
        evidence,
        "context-canary-mutant",
        context_test_command(),
        Duration::from_mins(3),
    )?;
    let stdout = fs::read_to_string(evidence.join("context-canary-mutant.stdout"))
        .map_err(|error| XtaskError::io("read context canary result", evidence, error))?;
    let detected = !outcome.timed_out
        && outcome.status.code() == Some(101)
        && stdout.contains(&format!("test {CONTEXT_TEST} ... FAILED"))
        && stdout.contains("test result: FAILED. 0 passed; 1 failed;");
    write_json(
        &evidence.join("context-canary-summary.json"),
        &json!({
            "status": if detected { "completed" } else { "failed_or_incomplete" },
            "baseline": "passed",
            "mutant": "remove hard available-token ceiling from required-closure growth",
            "test": CONTEXT_TEST,
            "behavioral_detection_demonstrated": detected,
            "mutant_exit_code": outcome.status.code(),
            "mutant_timed_out": outcome.timed_out,
        }),
    )?;
    if !detected {
        return Err(XtaskError::metadata(
            "context canary was not rejected by the exact owning test",
        ));
    }
    Ok(())
}

fn context_test_command() -> Command {
    let mut command = Command::new("cargo");
    command.args([
        "test",
        "--locked",
        "--offline",
        "--package",
        "peritus-context",
        "--test",
        "working_headroom",
        CONTEXT_TEST,
        "--",
        "--exact",
    ]);
    command
}

fn require_clean_tracked_source(root: &Path) -> Result<(), XtaskError> {
    for arguments in
        [&["diff", "--quiet", "HEAD"][..], &["diff", "--cached", "--quiet", "HEAD"][..]]
    {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .status()
            .map_err(|error| XtaskError::io("inspect context canary source", root, error))?;
        if !status.success() {
            return Err(XtaskError::metadata(
                "discovery campaign requires committed tracked source so its disposable clone is exact",
            ));
        }
    }
    Ok(())
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, XtaskError> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("capture context canary diff", root, error))?;
    if !output.status.success() {
        return Err(XtaskError::metadata("git could not capture context canary diff"));
    }
    Ok(output.stdout)
}

pub(super) fn run(
    root: &Path,
    evidence: &Path,
    index: usize,
    shard: Option<usize>,
) -> Result<(), XtaskError> {
    require_clean_tracked_source(root)?;
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
    let mut repository = MutationRepository::clone(root, evidence)?;
    let result = run_campaign(repository.path()?, evidence, index, shard, &selected);
    let cleanup = repository.remove();
    match result {
        Err(error) => Err(error),
        Ok(()) => cleanup,
    }
}

fn run_campaign(
    repository: &Path,
    evidence: &Path,
    index: usize,
    shard: Option<usize>,
    selected: &Value,
) -> Result<(), XtaskError> {
    let mut campaign = command(index)?;
    if let Some(shard) = shard {
        campaign.args(["--shard", &format!("{shard}/8")]);
    }
    campaign.args([
        "--in-place",
        "--baseline",
        "run",
        "--no-shuffle",
        "--build-timeout",
        "180",
        "--timeout",
        "60",
        "--output",
    ]);
    campaign.arg(evidence);
    environment::configure(&mut campaign, repository, evidence)?;
    let outcome = runner::run(repository, evidence, "mutation", campaign, Duration::from_mins(8))?;
    let path = evidence.join("mutants.out/outcomes.json");
    if !path.is_file() {
        return Err(XtaskError::metadata(format!(
            "mutation engine exited {} without an outcome report; inspect {}",
            outcome.status,
            evidence.display()
        )));
    }
    let outcomes =
        fs::read(&path).map_err(|error| XtaskError::io("read mutation outcomes", &path, error))?;
    let outcomes: Value = serde_json::from_slice(&outcomes).map_err(XtaskError::metadata_decode)?;
    let summary = summarize(selected, &outcomes)?;
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
