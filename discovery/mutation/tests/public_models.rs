//! Mutation calibration through the existing independent public API models.
use std::{
    env,
    fmt::Debug,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
};

use peritus_praxis::Metrics;
use proptest::test_runner::{Config, RngSeed, TestCaseError, TestRunner};
use serde::Serialize;

mod agent_driver_with_child {
    use std::path::Path;

    pub use peritus_praxis::agent_driver::{Case, examples, strategy};
    use peritus_praxis::{Metrics, agent_driver};

    pub fn check(case: &Case) -> Result<Metrics, String> {
        agent_driver::check_with_child(
            case,
            Path::new(env!("CARGO_BIN_EXE_peritus-mutation-agent-driver-child")),
        )
    }
}

fn number(name: &str, default: u32) -> u32 {
    env::var(name).map_or(default, |s| s.parse().expect("invalid mutation campaign setting"))
}

fn check_case<T: Debug + Serialize>(
    target: &str,
    case: &T,
    check: impl FnOnce(&T) -> Result<Metrics, String>,
) -> Result<(), String> {
    let result = catch_unwind(AssertUnwindSafe(|| check(case))).unwrap_or_else(|panic| {
        let detail = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("nontext panic");
        Err(format!("production or checker panic: {detail}"))
    });
    match result {
        Ok(_) => Ok(()),
        Err(error) => {
            if let Some(directory) = env::var_os("PERITUS_MUTATION_FAILURES") {
                let directory = PathBuf::from(directory);
                fs::create_dir_all(&directory).expect("create mutation failures directory");
                let value = serde_json::json!({"target":target,"input":case,"error":error});
                let bytes = serde_json::to_vec_pretty(&value).expect("serialize mutation witness");
                fs::write(directory.join(format!("{target}-{}.json", std::process::id())), bytes)
                    .expect("write mutation witness");
            }
            Err(format!("{target}: {error}; exact input: {case:?}"))
        }
    }
}

macro_rules! campaign {
    ($name:ident, $module:path) => {
        #[test]
        fn $name() {
            use $module as subject;
            let target = stringify!($name);
            if let Ok(selected) = env::var("PERITUS_MUTATION_TARGETS") {
                assert!(!selected.is_empty(), "empty mutation target selection");
                if !selected.split(',').any(|name| name == target) { return; }
            }
            let cases=number("PERITUS_MUTATION_CASES",64);
            assert!(cases>0 && cases<=1_000_000);
            let steps=number("PERITUS_MUTATION_STEPS",64);
            assert!(steps>0 && steps<=4096);
            let seed=u64::from(number("PERITUS_MUTATION_SEED",7319));
            let examples=subject::examples();
            assert!(!examples.is_empty(), "empty fixed-case campaign");
            for case in &examples {
                check_case(target,case,subject::check).unwrap_or_else(|error| panic!("{error}"));
            }
            let config=Config { cases, rng_seed:RngSeed::Fixed(seed), max_shrink_iters:4096, failure_persistence:None, ..Config::default() };
            let mut runner=TestRunner::new(config);
            runner.run(&subject::strategy(steps as usize),|case| {
                check_case(target,&case,subject::check).map_err(TestCaseError::fail)
            }).unwrap_or_else(|error| panic!("{target} seed={seed}: {error}"));
            eprintln!("MUTATION_CAMPAIGN target={target} fixed={} generated={cases} steps={steps} seed={seed}",examples.len());
        }
    };
}

campaign!(framing, peritus_praxis::framing);
campaign!(context, peritus_praxis::context);
campaign!(telemetry, peritus_praxis::telemetry);
campaign!(invalidation, peritus_praxis::invalidation);
campaign!(knowledge, peritus_praxis::knowledge);
campaign!(review, peritus_praxis::review);
campaign!(settlement, peritus_praxis::settlement);
campaign!(agent_driver, agent_driver_with_child);
campaign!(memory, peritus_praxis::memory);
campaign!(modules, peritus_praxis::stage4::modules);
campaign!(evidence, peritus_praxis::stage4::evidence);
campaign!(artifact_stream, peritus_praxis::stage4::artifact_stream);
campaign!(approval, peritus_praxis::stage4::approval);
campaign!(artifacts, peritus_praxis::stage4::artifacts);
campaign!(collaboration, peritus_praxis::stage4::collaboration);
campaign!(harness, peritus_praxis::stage4::harness);
campaign!(journal, peritus_praxis::stage4::journal);
campaign!(leases, peritus_praxis::stage4::leases);
campaign!(migrations, peritus_praxis::stage4::migrations);
campaign!(projection, peritus_praxis::stage4::projection);
campaign!(scheduler, peritus_praxis::stage4::scheduler);
campaign!(codec, peritus_praxis::stage5::codec);
campaign!(models, peritus_praxis::stage5::models);
campaign!(tools, peritus_praxis::stage5::tools);
campaign!(plugins, peritus_praxis::stage5::plugins);
campaign!(mcp, peritus_praxis::stage5::mcp);
campaign!(wire, peritus_praxis::stage5::wire);
campaign!(providers, peritus_praxis::stage5::providers);
campaign!(spec, peritus_praxis::stage6::spec);
campaign!(tui_paste, peritus_praxis::stage7::tui_paste);
campaign!(security, peritus_praxis::stage6::security);
campaign!(kernel, peritus_praxis::stage6::kernel);
campaign!(fs_dispatch, peritus_praxis::stage6::fs_dispatch);
campaign!(process_output, peritus_praxis::stage6::process_output);
campaign!(authority, peritus_praxis::stage6::authority);
campaign!(router, peritus_praxis::stage6::router);
campaign!(filesystem, peritus_praxis::stage6::filesystem);
campaign!(scoped_read, peritus_praxis::stage6::scoped_read);
campaign!(fs_snapshot, peritus_praxis::stage6::fs_snapshot);
campaign!(host, peritus_praxis::stage6::host);
campaign!(sandbox, peritus_praxis::stage6::sandbox);
campaign!(web_source, peritus_praxis::stage7::web_source);
