//! Thin composition root for retained `HarnessBench` campaign reports.

fn main() -> std::process::ExitCode {
    match peritus_external_benchmarks::harness_results::run_cli(std::env::args_os()) {
        Ok(summary) => match serde_json::to_string(&summary) {
            Ok(json) => {
                println!("{json}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("peritus-harnessbench-report: encode summary: {error}");
                std::process::ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("peritus-harnessbench-report: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
