//! Peritus production daemon executable.

fn main() -> std::process::ExitCode {
    peritus_daemon::run_cli(std::env::args_os())
}
