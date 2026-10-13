//! Real consumer of the production packaged-binary discovery API.
use std::{env, path::PathBuf, process::ExitCode};
use peritus_launcher::SiblingBinaries;

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!("usage: macos-command-path-consumer APPLICATION");
        return ExitCode::from(2);
    };
    match SiblingBinaries::from_application(PathBuf::from(path)) {
        Ok(pair) => {
            println!("{}", pair.application().display());
            println!("{}", pair.daemon().display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
