//! Thin native H2 qualification entry point.

use peritus_platform_qualification::H2OperatorStatus;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match peritus_platform_qualification::run_h2_operator()? {
        H2OperatorStatus::Ready => Ok(()),
        H2OperatorStatus::NotReady => std::process::exit(3),
    }
}
