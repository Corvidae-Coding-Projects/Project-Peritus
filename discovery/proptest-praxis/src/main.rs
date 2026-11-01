//! Reproducible property search and exact-case replay against real Peritus libraries.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    env, fs,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use peritus_praxis::{
    Metrics, agent_driver, context, framing, invalidation, knowledge, memory, review, settlement,
    stage4, stage5, stage6, stage7, telemetry,
};
use proptest::{
    strategy::{BoxedStrategy, Strategy},
    test_runner::{
        Config, FileFailurePersistence, RngAlgorithm, RngSeed, TestCaseError, TestError, TestRunner,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const SCHEMA_VERSION: u32 = 1;
const MAX_REPLAY_BYTES: u64 = 16 * 1024 * 1024;

mod fingerprint;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "target", content = "input", rename_all = "snake_case")]
enum Case {
    Framing(framing::Case),
    Context(context::Case),
    Telemetry(telemetry::Case),
    Invalidation(invalidation::Case),
    Knowledge(knowledge::Case),
    Review(review::Case),
    Settlement(settlement::Case),
    AgentDriver(agent_driver::Case),
    Memory(memory::Case),
    Modules(stage4::modules::Case),
    Evidence(stage4::evidence::Case),
    ArtifactStream(stage4::artifact_stream::Case),
    Codec(stage5::codec::Case),
    Models(stage5::models::Case),
    Tools(stage5::tools::Case),
    Plugins(stage5::plugins::Case),
    Mcp(stage5::mcp::Case),
    Wire(stage5::wire::Case),
    Providers(stage5::providers::Case),
    Spec(stage6::spec::Case),
    TuiPaste(stage7::tui_paste::Case),
    Security(stage6::security::Case),
    Kernel(stage6::kernel::Case),
    FsDispatch(stage6::fs_dispatch::Case),
    ProcessOutput(stage6::process_output::Case),
    Authority(stage6::authority::Case),
    Router(stage6::router::Case),
    Filesystem(stage6::filesystem::Case),
    ScopedRead(stage6::scoped_read::Case),
    FsSnapshot(stage6::fs_snapshot::Case),
    Host(stage6::host::Case),
    Sandbox(stage6::sandbox::Case),
    WebSource(stage7::web_source::Case),
    Approval(stage4::approval::Case),
    Artifacts(stage4::artifacts::Case),
    Collaboration(stage4::collaboration::Case),
    Harness(stage4::harness::Case),
    Journal(stage4::journal::Case),
    Leases(stage4::leases::Case),
    Migrations(stage4::migrations::Case),
    Projection(stage4::projection::Case),
    Scheduler(stage4::scheduler::Case),
}

impl Case {
    fn target(&self) -> &'static str {
        match self {
            Self::Framing(_) => "framing",
            Self::Context(_) => "context",
            Self::Telemetry(_) => "telemetry",
            Self::Invalidation(_) => "invalidation",
            Self::Knowledge(_) => "knowledge",
            Self::Review(_) => "review",
            Self::Settlement(_) => "settlement",
            Self::AgentDriver(_) => "agent_driver",
            Self::Memory(_) => "memory",
            Self::Modules(_) => "modules",
            Self::Evidence(_) => "evidence",
            Self::ArtifactStream(_) => "artifact_stream",
            Self::Codec(_) => "codec",
            Self::Models(_) => "models",
            Self::Tools(_) => "tools",
            Self::Plugins(_) => "plugins",
            Self::Mcp(_) => "mcp",
            Self::Wire(_) => "wire",
            Self::Providers(_) => "providers",
            Self::Spec(_) => "spec",
            Self::TuiPaste(_) => "tui_paste",
            Self::Security(_) => "security",
            Self::Kernel(_) => "kernel",
            Self::FsDispatch(_) => "fs_dispatch",
            Self::ProcessOutput(_) => "process_output",
            Self::Authority(_) => "authority",
            Self::Router(_) => "router",
            Self::Filesystem(_) => "filesystem",
            Self::ScopedRead(_) => "scoped_read",
            Self::FsSnapshot(_) => "fs_snapshot",
            Self::Host(_) => "host",
            Self::Sandbox(_) => "sandbox",
            Self::WebSource(_) => "web_source",
            Self::Approval(_) => "approval",
            Self::Artifacts(_) => "artifacts",
            Self::Collaboration(_) => "collaboration",
            Self::Harness(_) => "harness",
            Self::Journal(_) => "journal",
            Self::Leases(_) => "leases",
            Self::Migrations(_) => "migrations",
            Self::Projection(_) => "projection",
            Self::Scheduler(_) => "scheduler",
        }
    }

    fn check(&self) -> Result<Metrics, String> {
        let checked = catch_unwind(AssertUnwindSafe(|| match self {
            Self::Framing(case) => framing::check(case),
            Self::Context(case) => context::check(case),
            Self::Telemetry(case) => telemetry::check(case),
            Self::Invalidation(case) => invalidation::check(case),
            Self::Knowledge(case) => knowledge::check(case),
            Self::Review(case) => review::check(case),
            Self::Settlement(case) => settlement::check(case),
            Self::AgentDriver(case) => agent_driver::check(case),
            Self::Memory(case) => memory::check(case),
            Self::Modules(case) => stage4::modules::check(case),
            Self::Evidence(case) => stage4::evidence::check(case),
            Self::ArtifactStream(case) => stage4::artifact_stream::check(case),
            Self::Codec(case) => stage5::codec::check(case),
            Self::Models(case) => stage5::models::check(case),
            Self::Tools(case) => stage5::tools::check(case),
            Self::Plugins(case) => stage5::plugins::check(case),
            Self::Mcp(case) => stage5::mcp::check(case),
            Self::Wire(case) => stage5::wire::check(case),
            Self::Providers(case) => stage5::providers::check(case),
            Self::Spec(case) => stage6::spec::check(case),
            Self::TuiPaste(case) => stage7::tui_paste::check(case),
            Self::Security(case) => stage6::security::check(case),
            Self::Kernel(case) => stage6::kernel::check(case),
            Self::FsDispatch(case) => stage6::fs_dispatch::check(case),
            Self::ProcessOutput(case) => stage6::process_output::check(case),
            Self::Authority(case) => stage6::authority::check(case),
            Self::Router(case) => stage6::router::check(case),
            Self::Filesystem(case) => stage6::filesystem::check(case),
            Self::ScopedRead(case) => stage6::scoped_read::check(case),
            Self::FsSnapshot(case) => stage6::fs_snapshot::check(case),
            Self::Host(case) => stage6::host::check(case),
            Self::Sandbox(case) => stage6::sandbox::check(case),
            Self::WebSource(case) => stage7::web_source::check(case),
            Self::Approval(case) => stage4::approval::check(case),
            Self::Artifacts(case) => stage4::artifacts::check(case),
            Self::Collaboration(case) => stage4::collaboration::check(case),
            Self::Harness(case) => stage4::harness::check(case),
            Self::Journal(case) => stage4::journal::check(case),
            Self::Leases(case) => stage4::leases::check(case),
            Self::Migrations(case) => stage4::migrations::check(case),
            Self::Projection(case) => stage4::projection::check(case),
            Self::Scheduler(case) => stage4::scheduler::check(case),
        }));
        checked.unwrap_or_else(|panic| {
            let detail = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            Err(format!("production/consumer panic: {detail}"))
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Witness {
    schema_version: u32,
    generator_version: String,
    provenance: Value,
    seed: Option<u64>,
    expected: String,
    case: Case,
}

#[derive(Debug)]
struct Options {
    command: String,
    subject: String,
    output: PathBuf,
    profile: String,
    seed: u64,
    cases: u32,
    steps: usize,
}

fn options() -> Result<Options, String> {
    let mut args = env::args().skip(1);
    let usage = "usage: peritus-praxis <search|examples|calibration|replay> <framing|context|telemetry|CASE.json> --output NEW_DIRECTORY [--profile smoke|standard|extended] [--seed U64] [--cases COUNT] [--max-steps COUNT]";
    let command = args.next().ok_or(usage)?;
    let subject = args.next().ok_or(usage)?;
    if !matches!(command.as_str(), "search" | "examples" | "calibration" | "replay") {
        return Err(usage.into());
    }
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        if !matches!(key.as_str(), "--output" | "--profile" | "--seed" | "--cases" | "--max-steps")
        {
            return Err(format!("unknown option {key}; {usage}"));
        }
        let value = args.next().ok_or(usage)?;
        if values.insert(key, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let profile = values.get("--profile").cloned().unwrap_or_else(|| "smoke".into());
    let (default_cases, default_steps) = match profile.as_str() {
        "smoke" => (256, 64),
        "standard" => (10_000, 256),
        "extended" => (100_000, 2048),
        _ => return Err("unknown profile".into()),
    };
    let parse = |key: &str, default: u64| -> Result<u64, String> {
        values
            .get(key)
            .map_or(Ok(default), |value| value.parse().map_err(|_| format!("invalid {key}")))
    };
    let cases =
        u32::try_from(parse("--cases", default_cases)?).map_err(|_| "case count overflow")?;
    let steps =
        usize::try_from(parse("--max-steps", default_steps)?).map_err(|_| "step count overflow")?;
    if cases == 0 || cases > 1_000_000 || steps == 0 || steps > 4096 {
        return Err("require 1..=1000000 cases and 1..=4096 steps".into());
    }
    Ok(Options {
        command,
        subject,
        output: PathBuf::from(values.get("--output").ok_or(usage)?),
        profile,
        seed: parse("--seed", 7319)?,
        cases,
        steps,
    })
}

fn provenance() -> Result<Value, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let built: Value =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/build-inputs.json")))
            .map_err(|e| e.to_string())?;
    let current = fingerprint::snapshot(manifest)?;
    for field in ["production_source_sha256", "harness_sha256"] {
        if built[field] != current[field] {
            return Err(format!(
                "stale binary: {field} differs from compiled inputs; rebuild before running"
            ));
        }
    }
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    Ok(json!({
        "compiled_inputs": built, "runtime_inputs": current,
        "executable_sha256": fingerprint::digest(&fs::read(executable).map_err(|e| e.to_string())?),
        "os": env::consts::OS, "arch": env::consts::ARCH,
        "proptest": "1.12.0", "rng_algorithm": "ChaCha",
    }))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}

fn witness(case: Case, seed: Option<u64>, provenance: &Value) -> Witness {
    Witness {
        schema_version: SCHEMA_VERSION, generator_version: "strengthening-v1".into(),
        provenance: provenance.clone(), seed,
        expected: "The independent oracle and the actual public Peritus API must agree on this exact input.".into(),
        case,
    }
}

fn strategy(target: &str, steps: usize) -> Result<BoxedStrategy<Case>, String> {
    Ok(match target {
        "framing" => framing::strategy(steps).prop_map(Case::Framing).boxed(),
        "context" => context::strategy(steps).prop_map(Case::Context).boxed(),
        "telemetry" => telemetry::strategy(steps).prop_map(Case::Telemetry).boxed(),
        "invalidation" => invalidation::strategy(steps).prop_map(Case::Invalidation).boxed(),
        "knowledge" => knowledge::strategy(steps).prop_map(Case::Knowledge).boxed(),
        "review" => review::strategy(steps).prop_map(Case::Review).boxed(),
        "settlement" => settlement::strategy(steps).prop_map(Case::Settlement).boxed(),
        "agent_driver" => agent_driver::strategy(steps).prop_map(Case::AgentDriver).boxed(),
        "memory" => memory::strategy(steps).prop_map(Case::Memory).boxed(),
        "modules" => stage4::modules::strategy(steps).prop_map(Case::Modules).boxed(),
        "evidence" => stage4::evidence::strategy(steps).prop_map(Case::Evidence).boxed(),
        "artifact_stream" => {
            stage4::artifact_stream::strategy(steps).prop_map(Case::ArtifactStream).boxed()
        }
        "approval" => stage4::approval::strategy(steps).prop_map(Case::Approval).boxed(),
        "artifacts" => stage4::artifacts::strategy(steps).prop_map(Case::Artifacts).boxed(),
        "collaboration" => {
            stage4::collaboration::strategy(steps).prop_map(Case::Collaboration).boxed()
        }
        "harness" => stage4::harness::strategy(steps).prop_map(Case::Harness).boxed(),
        "journal" => stage4::journal::strategy(steps).prop_map(Case::Journal).boxed(),
        "leases" => stage4::leases::strategy(steps).prop_map(Case::Leases).boxed(),
        "migrations" => stage4::migrations::strategy(steps).prop_map(Case::Migrations).boxed(),
        "projection" => stage4::projection::strategy(steps).prop_map(Case::Projection).boxed(),
        "scheduler" => stage4::scheduler::strategy(steps).prop_map(Case::Scheduler).boxed(),
        "codec" => stage5::codec::strategy(steps).prop_map(Case::Codec).boxed(),
        "models" => stage5::models::strategy(steps).prop_map(Case::Models).boxed(),
        "tools" => stage5::tools::strategy(steps).prop_map(Case::Tools).boxed(),
        "plugins" => stage5::plugins::strategy(steps).prop_map(Case::Plugins).boxed(),
        "mcp" => stage5::mcp::strategy(steps).prop_map(Case::Mcp).boxed(),
        "wire" => stage5::wire::strategy(steps).prop_map(Case::Wire).boxed(),
        "providers" => stage5::providers::strategy(steps).prop_map(Case::Providers).boxed(),
        "spec" => stage6::spec::strategy(steps).prop_map(Case::Spec).boxed(),
        "tui_paste" => stage7::tui_paste::strategy(steps).prop_map(Case::TuiPaste).boxed(),
        "security" => stage6::security::strategy(steps).prop_map(Case::Security).boxed(),
        "kernel" => stage6::kernel::strategy(steps).prop_map(Case::Kernel).boxed(),
        "fs_dispatch" => stage6::fs_dispatch::strategy(steps).prop_map(Case::FsDispatch).boxed(),
        "process_output" => {
            stage6::process_output::strategy(steps).prop_map(Case::ProcessOutput).boxed()
        }
        "authority" => stage6::authority::strategy(steps).prop_map(Case::Authority).boxed(),
        "router" => stage6::router::strategy(steps).prop_map(Case::Router).boxed(),
        "filesystem" => stage6::filesystem::strategy(steps).prop_map(Case::Filesystem).boxed(),
        "scoped_read" => stage6::scoped_read::strategy(steps).prop_map(Case::ScopedRead).boxed(),
        "fs_snapshot" => stage6::fs_snapshot::strategy(steps).prop_map(Case::FsSnapshot).boxed(),
        "host" => stage6::host::strategy(steps).prop_map(Case::Host).boxed(),
        "sandbox" => stage6::sandbox::strategy(steps).prop_map(Case::Sandbox).boxed(),
        "web_source" => stage7::web_source::strategy(steps).prop_map(Case::WebSource).boxed(),
        _ => return Err(format!("unknown target {target}")),
    })
}

fn explicit(target: &str, calibration: bool) -> Result<Vec<Case>, String> {
    Ok(match (target, calibration) {
        ("framing", false) => framing::examples().into_iter().map(Case::Framing).collect(),
        ("context", false) => context::examples().into_iter().map(Case::Context).collect(),
        ("telemetry", false) => telemetry::examples().into_iter().map(Case::Telemetry).collect(),
        ("framing", true) => vec![Case::Framing(framing::calibration_case())],
        ("context", true) => vec![Case::Context(context::calibration_case())],
        ("telemetry", true) => vec![Case::Telemetry(telemetry::calibration_case())],
        ("invalidation", false) => {
            invalidation::examples().into_iter().map(Case::Invalidation).collect()
        }
        ("knowledge", false) => knowledge::examples().into_iter().map(Case::Knowledge).collect(),
        ("memory", false) => memory::examples().into_iter().map(Case::Memory).collect(),
        ("invalidation", true) => vec![Case::Invalidation(invalidation::calibration_case())],
        ("knowledge", true) => vec![Case::Knowledge(knowledge::calibration_case())],
        ("review", false) => review::examples().into_iter().map(Case::Review).collect(),
        ("review", true) => vec![Case::Review(review::calibration_case())],
        ("settlement", false) => settlement::examples().into_iter().map(Case::Settlement).collect(),
        ("settlement", true) => vec![Case::Settlement(settlement::calibration_case())],
        ("agent_driver", false) => {
            agent_driver::examples().into_iter().map(Case::AgentDriver).collect()
        }
        ("agent_driver", true) => vec![Case::AgentDriver(agent_driver::calibration_case())],
        ("memory", true) => vec![Case::Memory(memory::calibration_case())],
        ("modules", false) => stage4::modules::examples().into_iter().map(Case::Modules).collect(),
        ("modules", true) => vec![Case::Modules(stage4::modules::calibration_case())],
        ("evidence", false) => {
            stage4::evidence::examples().into_iter().map(Case::Evidence).collect()
        }
        ("evidence", true) => vec![Case::Evidence(stage4::evidence::calibration())],
        ("artifact_stream", false) => {
            stage4::artifact_stream::examples().into_iter().map(Case::ArtifactStream).collect()
        }
        ("artifact_stream", true) => {
            vec![Case::ArtifactStream(stage4::artifact_stream::calibration())]
        }
        ("approval", false) => {
            stage4::approval::examples().into_iter().map(Case::Approval).collect()
        }
        ("approval", true) => vec![Case::Approval(stage4::approval::calibration())],
        ("artifacts", false) => {
            stage4::artifacts::examples().into_iter().map(Case::Artifacts).collect()
        }
        ("artifacts", true) => vec![Case::Artifacts(stage4::artifacts::calibration_case())],
        ("collaboration", false) => {
            stage4::collaboration::examples().into_iter().map(Case::Collaboration).collect()
        }
        ("collaboration", true) => vec![Case::Collaboration(stage4::collaboration::calibration())],
        ("harness", false) => stage4::harness::examples().into_iter().map(Case::Harness).collect(),
        ("harness", true) => vec![Case::Harness(stage4::harness::calibration())],
        ("journal", false) => stage4::journal::examples().into_iter().map(Case::Journal).collect(),
        ("journal", true) => vec![Case::Journal(stage4::journal::calibration())],
        ("leases", false) => stage4::leases::examples().into_iter().map(Case::Leases).collect(),
        ("leases", true) => vec![Case::Leases(stage4::leases::calibration())],
        ("migrations", false) => {
            stage4::migrations::examples().into_iter().map(Case::Migrations).collect()
        }
        ("migrations", true) => vec![Case::Migrations(stage4::migrations::calibration())],
        ("projection", false) => {
            stage4::projection::examples().into_iter().map(Case::Projection).collect()
        }
        ("projection", true) => vec![Case::Projection(stage4::projection::calibration())],
        ("scheduler", false) => {
            stage4::scheduler::examples().into_iter().map(Case::Scheduler).collect()
        }
        ("scheduler", true) => vec![Case::Scheduler(stage4::scheduler::calibration())],
        ("codec", false) => stage5::codec::examples().into_iter().map(Case::Codec).collect(),
        ("codec", true) => vec![Case::Codec(stage5::codec::calibration())],
        ("models", false) => stage5::models::examples().into_iter().map(Case::Models).collect(),
        ("models", true) => vec![Case::Models(stage5::models::calibration())],
        ("tools", false) => stage5::tools::examples().into_iter().map(Case::Tools).collect(),
        ("tools", true) => vec![Case::Tools(stage5::tools::calibration())],
        ("wire", false) => stage5::wire::examples().into_iter().map(Case::Wire).collect(),
        ("wire", true) => vec![Case::Wire(stage5::wire::calibration())],
        ("providers", false) => {
            stage5::providers::examples().into_iter().map(Case::Providers).collect()
        }
        ("providers", true) => vec![Case::Providers(stage5::providers::calibration())],
        ("mcp", false) => stage5::mcp::examples().into_iter().map(Case::Mcp).collect(),
        ("mcp", true) => vec![Case::Mcp(stage5::mcp::calibration())],
        ("plugins", false) => stage5::plugins::examples().into_iter().map(Case::Plugins).collect(),
        ("plugins", true) => vec![Case::Plugins(stage5::plugins::calibration())],
        ("spec", false) => stage6::spec::examples().into_iter().map(Case::Spec).collect(),
        ("spec", true) => vec![Case::Spec(stage6::spec::calibration())],
        ("tui_paste", false) => {
            stage7::tui_paste::examples().into_iter().map(Case::TuiPaste).collect()
        }
        ("tui_paste", true) => vec![Case::TuiPaste(stage7::tui_paste::calibration())],
        ("security", false) => {
            stage6::security::examples().into_iter().map(Case::Security).collect()
        }
        ("security", true) => vec![Case::Security(stage6::security::calibration())],
        ("kernel", false) => stage6::kernel::examples().into_iter().map(Case::Kernel).collect(),
        ("kernel", true) => vec![Case::Kernel(stage6::kernel::calibration())],
        ("fs_dispatch", false) => {
            stage6::fs_dispatch::examples().into_iter().map(Case::FsDispatch).collect()
        }
        ("fs_dispatch", true) => vec![Case::FsDispatch(stage6::fs_dispatch::calibration_case())],
        ("process_output", false) => {
            stage6::process_output::examples().into_iter().map(Case::ProcessOutput).collect()
        }
        ("process_output", true) => {
            vec![Case::ProcessOutput(stage6::process_output::calibration())]
        }
        ("authority", false) => {
            stage6::authority::examples().into_iter().map(Case::Authority).collect()
        }
        ("authority", true) => vec![Case::Authority(stage6::authority::calibration())],
        ("router", false) => stage6::router::examples().into_iter().map(Case::Router).collect(),
        ("router", true) => vec![Case::Router(stage6::router::calibration())],
        ("filesystem", false) => {
            stage6::filesystem::examples().into_iter().map(Case::Filesystem).collect()
        }
        ("filesystem", true) => vec![Case::Filesystem(stage6::filesystem::calibration_case())],
        ("scoped_read", false) => {
            stage6::scoped_read::examples().into_iter().map(Case::ScopedRead).collect()
        }
        ("scoped_read", true) => vec![Case::ScopedRead(stage6::scoped_read::calibration_case())],
        ("fs_snapshot", false) => {
            stage6::fs_snapshot::examples().into_iter().map(Case::FsSnapshot).collect()
        }
        ("fs_snapshot", true) => vec![Case::FsSnapshot(stage6::fs_snapshot::calibration_case())],
        ("host", false) => stage6::host::examples().into_iter().map(Case::Host).collect(),
        ("host", true) => vec![Case::Host(stage6::host::calibration())],
        ("sandbox", false) => stage6::sandbox::examples().into_iter().map(Case::Sandbox).collect(),
        ("sandbox", true) => vec![Case::Sandbox(stage6::sandbox::calibration())],
        ("web_source", false) => {
            stage7::web_source::examples().into_iter().map(Case::WebSource).collect()
        }
        ("web_source", true) => vec![Case::WebSource(stage7::web_source::calibration())],
        _ => return Err(format!("unknown target {target}")),
    })
}

fn add_metrics(total: &mut Metrics, next: Metrics) {
    for (name, count) in next {
        let maximum = name.starts_with("maximum_") || name.starts_with("max_");
        let aggregate = total.entry(name).or_default();
        if maximum {
            *aggregate = (*aggregate).max(count);
        } else {
            *aggregate = aggregate.saturating_add(count);
        }
    }
}

fn peak_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:").and_then(|value| value.split_whitespace().next()?.parse().ok())
    })
}

fn execute(options: Options) -> Result<bool, String> {
    fs::create_dir(&options.output)
        .map_err(|e| format!("output must be a new directory {}: {e}", options.output.display()))?;
    let started = Instant::now();
    let provenance = provenance()?;
    let metrics = RefCell::new(Metrics::new());
    let checks = RefCell::new(0_u64);
    let failure = RefCell::new(None::<String>);
    let mut passes = 0_u64;

    if options.command == "search" {
        let saved_first = RefCell::new(false);
        let persistence = options
            .output
            .join("proptest-seeds.txt")
            .to_str()
            .ok_or("output path must be UTF-8")?
            .to_owned();
        let config = Config {
            cases: options.cases,
            rng_seed: RngSeed::Fixed(options.seed),
            rng_algorithm: RngAlgorithm::ChaCha,
            // Setup failures are missing execution, never discardable input cases.
            max_global_rejects: 0,
            max_shrink_iters: 10_000,
            max_shrink_time: 30_000,
            failure_persistence: Some(Box::new(FileFailurePersistence::Direct(Box::leak(
                persistence.into_boxed_str(),
            )))),
            ..Config::default()
        };
        let mut runner = TestRunner::new(config);
        let result = runner.run(&strategy(&options.subject, options.steps)?, |case| {
            *checks.borrow_mut() += 1;
            match case.check() {
                Ok(next) => {
                    add_metrics(&mut metrics.borrow_mut(), next);
                    Ok(())
                }
                Err(reason) => {
                    if reason.starts_with("setup-incomplete:") {
                        write_json(
                            &options.output.join("incomplete-case.json"),
                            &witness(case.clone(), Some(options.seed), &provenance),
                        )
                        .map_err(TestCaseError::fail)?;
                        return Err(TestCaseError::reject(reason));
                    }
                    if !*saved_first.borrow() {
                        write_json(
                            &options.output.join("first-failure.json"),
                            &witness(case.clone(), Some(options.seed), &provenance),
                        )
                        .map_err(TestCaseError::fail)?;
                        *saved_first.borrow_mut() = true;
                    }
                    Err(TestCaseError::fail(reason))
                }
            }
        });
        match result {
            Ok(()) => passes = u64::from(options.cases),
            Err(TestError::Fail(reason, case)) => {
                write_json(
                    &options.output.join("minimized-failure.json"),
                    &witness(case, Some(options.seed), &provenance),
                )?;
                *failure.borrow_mut() = Some(reason.to_string());
            }
            Err(TestError::Abort(reason)) => return Err(format!("search incomplete: {reason}")),
        }
    } else {
        let cases = if options.command == "replay" {
            let path = Path::new(&options.subject);
            if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_REPLAY_BYTES {
                return Err("replay exceeds 16 MiB".into());
            }
            let value: Witness =
                serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if value.schema_version != SCHEMA_VERSION
                || !matches!(
                    value.generator_version.as_str(),
                    "stage1-v1"
                        | "stage2-v1"
                        | "stage3-v1"
                        | "stage4-v1"
                        | "stage5-v1"
                        | "stage6-v1"
                        | "stage7-v1"
                        | "strengthening-v1"
                )
            {
                return Err("unsupported replay schema/generator version".into());
            }
            vec![value.case]
        } else {
            explicit(&options.subject, options.command == "calibration")?
        };
        for (index, case) in cases.into_iter().enumerate() {
            write_json(
                &options.output.join(format!("case-{index:03}.json")),
                &witness(case.clone(), None, &provenance),
            )?;
            *checks.borrow_mut() += 1;
            match case.check() {
                Ok(next) => {
                    passes += 1;
                    add_metrics(&mut metrics.borrow_mut(), next);
                }
                Err(reason) => {
                    if reason.starts_with("setup-incomplete:") {
                        return Err(reason);
                    }
                    *failure.borrow_mut() =
                        Some(format!("case {index} ({}): {reason}", case.target()));
                    break;
                }
            }
        }
    }
    let passed = failure.borrow().is_none();
    let report = json!({
        "schema_version": SCHEMA_VERSION, "generator_version": "strengthening-v1",
        "command": options.command, "subject": options.subject, "profile": options.profile,
        "seed": options.seed, "requested_cases": options.cases, "max_steps": options.steps,
        "status": if passed {"pass"} else {"fail"}, "completed_cases": passes,
        "checker_invocations_including_shrinking": *checks.borrow(),
        "metrics": *metrics.borrow(), "failure": *failure.borrow(),
        "elapsed_seconds": started.elapsed().as_secs_f64(), "peak_rss_kib": peak_rss_kib(),
        "provenance": provenance, "metric_semantics": "Event totals over successful checker invocations; maximum_/max_ gauges use max. Shrinking can repeat cases. See target contract for each counter.",
    });
    write_json(&options.output.join("report.json"), &report)?;
    println!("{}", serde_json::to_string(&report).map_err(|e| e.to_string())?);
    Ok(passed)
}

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|v| v == "stage3-driver-child") {
        return match agent_driver::child_main(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("driver child failed: {e}");
                ExitCode::from(2)
            }
        };
    }
    match options().and_then(execute) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("incomplete: {error}");
            ExitCode::from(2)
        }
    }
}
