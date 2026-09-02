//! Thin final H0 aggregation entry point.

use peritus_security_qualification::H0AggregateStatus;

fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    match peritus_security_qualification::run_h0_aggregate_operator()? {
        H0AggregateStatus::Ready => Ok(std::process::ExitCode::SUCCESS),
        H0AggregateStatus::NotReady => Ok(std::process::ExitCode::FAILURE),
    }
}
