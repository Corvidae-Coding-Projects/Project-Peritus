//! Reviewed native H0 probe controller entry point.

use peritus_security_qualification::ControllerStatus;

fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    match peritus_security_qualification::run_h0_controller()? {
        ControllerStatus::Passed => Ok(std::process::ExitCode::SUCCESS),
        ControllerStatus::Failed => Ok(std::process::ExitCode::FAILURE),
    }
}
