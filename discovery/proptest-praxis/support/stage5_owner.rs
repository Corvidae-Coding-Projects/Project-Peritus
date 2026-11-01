//! Discovery-only entrypoint. Checks the actual owner crate; never exported by release builds.
use proptest::{
    strategy::BoxedStrategy,
    test_runner::{
        Config, FileFailurePersistence, RngAlgorithm, RngSeed, TestCaseError, TestError, TestRunner,
    },
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, env, fmt::Debug, fs, path::Path, time::Instant};

use super::praxis_fingerprint as fingerprint;

fn save(path: &Path, value: &impl Serialize) -> Result<(), String> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}

pub fn run<C: Clone + Debug + Serialize + DeserializeOwned>(
    target: &str,
    strategy: fn(usize) -> BoxedStrategy<C>,
    check: fn(&C) -> Result<BTreeMap<String, u64>, String>,
    examples: fn() -> Vec<C>,
    calibration: fn() -> C,
) {
    // An ordinary unconfigured cargo test runs explicit examples. Campaign commands add
    // a fresh output directory, bounded search controls and a verified build receipt.
    if env::var_os("PRAXIS_OUTPUT").is_none() {
        for case in examples() {
            check(&case).unwrap_or_else(|e| panic!("{target}: {case:?}: {e}"));
        }
        return;
    }
    let result = execute(target, strategy, check, examples, calibration);
    assert!(result.is_ok(), "praxis incomplete: {:?}", result.err());
    assert!(
        result.unwrap_or(false),
        "praxis behavioral failure; retained report and exact witnesses"
    );
}

fn execute<C: Clone + Debug + Serialize + DeserializeOwned>(
    target: &str,
    strategy: fn(usize) -> BoxedStrategy<C>,
    check: fn(&C) -> Result<BTreeMap<String, u64>, String>,
    examples: fn() -> Vec<C>,
    calibration: fn() -> C,
) -> Result<bool, String> {
    let output = std::path::PathBuf::from(env::var("PRAXIS_OUTPUT").map_err(|e| e.to_string())?);
    fs::create_dir(&output).map_err(|e| e.to_string())?;
    let command = env::var("PRAXIS_COMMAND").unwrap_or_else(|_| "examples".into());
    let (seed, cases, steps) = bounds()?;
    let provenance = provenance()?;
    let witness = |case: &C| json!({"schema_version":1,"generator_version":"stage5-owner-v1","target":target,"case":case,"seed":seed,"provenance":provenance,"expected":"Independent provider-boundary model agrees with actual adapter and decoder"});
    let totals = RefCell::new(BTreeMap::<String, u64>::new());
    let invocations = RefCell::new(0u64);
    let checked = |case: &C| -> Result<(), String> {
        *invocations.borrow_mut() += 1;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(case)))
            .unwrap_or_else(|panic| {
                Err(format!(
                    "checker/production panic: {}",
                    panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied())
                        .unwrap_or("non-string")
                ))
            });
        let metrics = result?;
        for (key, count) in metrics {
            let max = key.starts_with("max_") || key.starts_with("maximum_");
            let mut totals = totals.borrow_mut();
            let entry = totals.entry(key).or_default();
            *entry = if max { (*entry).max(count) } else { entry.saturating_add(count) };
        }
        Ok(())
    };
    let started = Instant::now();
    let mut failure = None;
    let mut completed = 0;
    if command == "search" {
        let mut runner = runner(&output, cases, seed);
        let first = RefCell::new(false);
        match runner.run(&strategy(steps), |case| {
            checked(&case).map_err(|reason| {
                if reason.starts_with("setup-incomplete:") {
                    save(&output.join("incomplete-case.json"), &witness(&case))
                        .expect("persist incomplete case");
                    return TestCaseError::reject(reason);
                }
                if !*first.borrow() {
                    save(&output.join("first-failure.json"), &witness(&case))
                        .expect("persist first witness");
                    *first.borrow_mut() = true;
                }
                TestCaseError::fail(reason)
            })
        }) {
            Ok(()) => completed = u64::from(cases),
            Err(TestError::Fail(reason, case)) => {
                save(&output.join("minimized-failure.json"), &witness(&case))?;
                failure = Some(reason.to_string());
            }
            Err(TestError::Abort(reason)) => return Err(format!("search incomplete: {reason}")),
        }
    } else {
        let inputs = match command.as_str() {
            "examples" => examples(),
            "calibration" => vec![calibration()],
            "replay" => {
                let path = env::var("PRAXIS_CASE").map_err(|e| e.to_string())?;
                if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
                    return Err("replay too large".into());
                }
                let v: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                if v["schema_version"] != 1
                    || v["generator_version"] != "stage5-owner-v1"
                    || v["target"] != target
                {
                    return Err("unsupported replay schema/target".into());
                }
                vec![serde_json::from_value(v["case"].clone()).map_err(|e| e.to_string())?]
            }
            _ => return Err("unknown campaign command".into()),
        };
        for (i, case) in inputs.iter().enumerate() {
            save(&output.join(format!("case-{i:03}.json")), &witness(case))?;
            match checked(case) {
                Ok(()) => completed += 1,
                Err(reason) => {
                    if reason.starts_with("setup-incomplete:") {
                        return Err(reason);
                    }
                    failure = Some(reason);
                    break;
                }
            }
        }
    }
    let passed = failure.is_none();
    let report = json!({"schema_version":1,"generator_version":"stage5-owner-v1","target":target,"command":command,"status":if passed {"pass"}else{"fail"},"seed":seed,"requested_cases":cases,"max_steps":steps,"completed_cases":completed,"checker_invocations_including_shrinking":*invocations.borrow(),"metrics":*totals.borrow(),"failure":failure,"elapsed_seconds":started.elapsed().as_secs_f64(),"provenance":provenance,"metric_semantics":"Event totals over successful checks; max_ gauges are maxima; shrinking may repeat cases."});
    save(&output.join("report.json"), &report)?;
    println!("{report}");
    Ok(passed)
}

fn provenance() -> Result<Value, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../discovery/proptest-praxis");
    let current = fingerprint::snapshot(&manifest)?;
    let receipt: Value = serde_json::from_slice(
        &fs::read(env::var("PRAXIS_RECEIPT").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for key in ["production_source_sha256", "harness_sha256"] {
        if current[key] != receipt["inputs"][key] {
            return Err(format!("stale owner binary: {key}"));
        }
    }
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    if fingerprint::digest(&fs::read(executable).map_err(|e| e.to_string())?)
        != receipt["executable_sha256"].as_str().ok_or("missing executable identity")?
    {
        return Err("owner executable does not match build receipt".into());
    }
    let provenance = json!({"build_receipt":receipt,"runtime_inputs":current});
    Ok(provenance)
}

fn runner(output: &Path, cases: u32, seed: u64) -> TestRunner {
    let persistence = output.join("proptest-seeds.txt").to_string_lossy().into_owned();
    TestRunner::new(Config {
        cases,
        rng_seed: RngSeed::Fixed(seed),
        rng_algorithm: RngAlgorithm::ChaCha,
        max_global_rejects: 0,
        max_shrink_iters: 10_000,
        max_shrink_time: 30_000,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(Box::leak(
            persistence.into_boxed_str(),
        )))),
        ..Config::default()
    })
}

fn bounds() -> Result<(u64, u32, usize), String> {
    let number = |key: &str, default: u64| -> Result<u64, String> {
        env::var(key).map_or(Ok(default), |v| v.parse().map_err(|_| format!("invalid {key}")))
    };
    let seed = number("PRAXIS_SEED", 7319)?;
    let cases = u32::try_from(number("PRAXIS_CASES", 256)?).map_err(|e| e.to_string())?;
    let steps = usize::try_from(number("PRAXIS_STEPS", 64)?).map_err(|e| e.to_string())?;
    if cases == 0 || cases > 1_000_000 || steps == 0 || steps > 4096 {
        return Err("invalid campaign bounds".into());
    }
    Ok((seed, cases, steps))
}
