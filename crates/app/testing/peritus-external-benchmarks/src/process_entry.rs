//! Process lifecycle and standard-stream handling for the native benchmark executable.

use std::{io::Read as _, process::ExitCode};

/// Runs the benchmark process using the current arguments and standard streams.
#[must_use]
pub fn main_entry() -> ExitCode {
    let runtime =
        match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("peritus-benchmark-agent: create runtime: {error}");
                return ExitCode::FAILURE;
            }
        };
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("rubric")) {
        return rubric(&runtime);
    }
    match runtime.block_on(crate::run(std::env::args_os())) {
        Ok(report) => {
            match serde_json::to_string_pretty(&report) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("peritus-benchmark-agent: encode report: {error}");
                    return ExitCode::FAILURE;
                }
            }
            if report.success { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(error) => {
            eprintln!("peritus-benchmark-agent: {error}");
            ExitCode::FAILURE
        }
    }
}

fn rubric(runtime: &tokio::runtime::Runtime) -> ExitCode {
    let mut body = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut body) {
        eprintln!("peritus-benchmark-agent: read rubric request: {error}");
        return ExitCode::FAILURE;
    }
    match runtime.block_on(crate::complete_rubric(&body)) {
        Ok(response) => match serde_json::to_string(&response) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("peritus-benchmark-agent: encode rubric response: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("peritus-benchmark-agent: rubric: {error}");
            ExitCode::FAILURE
        }
    }
}
