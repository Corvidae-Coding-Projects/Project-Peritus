#![doc = "Binary entry point for Peritus workspace policy checks."]

use std::process::ExitCode;

fn main() -> ExitCode {
    match xtask::run_from_env() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.render());
            if error.code() == xtask::ErrorCode::Invocation {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
