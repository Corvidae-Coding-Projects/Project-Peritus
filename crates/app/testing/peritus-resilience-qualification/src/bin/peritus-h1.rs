//! Thin native H1 qualification entry point.

use peritus_resilience_qualification::H1OperatorStatus;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match peritus_resilience_qualification::run_h1_operator()? {
        H1OperatorStatus::Ready | H1OperatorStatus::DiagnosticPassed => Ok(()),
        H1OperatorStatus::NotReady => std::process::exit(3),
    }
}
