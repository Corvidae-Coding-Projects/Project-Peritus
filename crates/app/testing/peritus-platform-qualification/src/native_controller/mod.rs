//! Real host-native checks behind the H2 controller protocol.

mod args;
mod checks;
mod request;
mod response;

use std::env;

use args::ControllerPaths;
use request::BoundRequest;

/// Runs one native H2 scenario from the adapter-supplied process arguments.
///
/// # Errors
///
/// Returns strict argument, request-binding, host-effect, or response-publication errors.
pub fn run_from_env() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let paths = ControllerPaths::parse(&arguments)?;
    let request = BoundRequest::load_and_validate(&paths)?;
    let observation = checks::run(&paths, &request)?;
    response::publish(&paths, &request, &observation)?;
    Ok(())
}
