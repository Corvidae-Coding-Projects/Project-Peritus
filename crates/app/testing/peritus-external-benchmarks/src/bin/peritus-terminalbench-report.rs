//! Thin composition root for retained Terminal-Bench campaign reports.

fn main() -> std::process::ExitCode {
    match peritus_external_benchmarks::terminal_results::run_cli(std::env::args_os()) {
        Ok(summary) => match serde_json::to_string(&summary) {
            Ok(json) => {
                println!("{json}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("peritus-terminalbench-report: encode summary: {error}");
                std::process::ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("peritus-terminalbench-report: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
