//! Thin native H0 shard entry point.

use peritus_security_qualification::H0OperatorStatus;

fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    match peritus_security_qualification::run_h0_operator()? {
        H0OperatorStatus::Passed => Ok(std::process::ExitCode::SUCCESS),
        H0OperatorStatus::Failed => Ok(std::process::ExitCode::FAILURE),
    }
}
