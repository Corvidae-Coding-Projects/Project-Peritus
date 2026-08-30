//! Persistent production controller for native H1 candidate effects.

mod args;
mod candidate;
mod evidence;
mod request;
mod response;
mod session;

use std::env;

use args::ControllerPaths;

/// Runs the strict persistent H1 controller protocol from process arguments and standard input.
///
/// # Errors
///
/// Returns argument, identity, protocol, candidate-effect, evidence, or output failures.
pub fn run_from_env() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let paths = ControllerPaths::parse(&arguments)?;
    session::serve(&paths)
}
