use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().map(String::as_str) != Some("stage3-driver-child") {
        eprintln!("expected stage3-driver-child CASE.json DIRECTORY");
        return ExitCode::from(2);
    }
    match peritus_praxis::agent_driver::child_main(&arguments[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("driver child failed: {error}");
            ExitCode::from(2)
        }
    }
}
