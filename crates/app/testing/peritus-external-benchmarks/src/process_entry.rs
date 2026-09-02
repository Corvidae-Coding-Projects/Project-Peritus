//! Process lifecycle and standard-stream handling for the native benchmark executable.

use std::{io::Read as _, process::ExitCode};

/// Runs the benchmark process using the current arguments and standard streams.
#[must_use]
pub fn main_entry() -> ExitCode {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("protocol")) {
        return protocol();
    }
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
            completed_attempt_exit(report.success())
        }
        Err(error) => {
            eprintln!("peritus-benchmark-agent: {error}");
            ExitCode::FAILURE
        }
    }
}

fn protocol() -> ExitCode {
    if std::env::args_os().count() != 2 {
        eprintln!("peritus-benchmark-agent: protocol does not accept arguments");
        return ExitCode::FAILURE;
    }
    match crate::identity::BenchmarkAgentProtocol::current()
        .and_then(|protocol| serde_json::to_string(&protocol).map_err(crate::BenchmarkError::from))
    {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("peritus-benchmark-agent: protocol: {error}");
            ExitCode::FAILURE
        }
    }
}

const fn completed_attempt_exit(product_accepted: bool) -> ExitCode {
    match product_accepted {
        true | false => ExitCode::SUCCESS,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_external_attempt_is_scoreable_even_when_product_rejects_it() {
        assert_eq!(completed_attempt_exit(true), ExitCode::SUCCESS);
        assert_eq!(completed_attempt_exit(false), ExitCode::SUCCESS);
    }
}
