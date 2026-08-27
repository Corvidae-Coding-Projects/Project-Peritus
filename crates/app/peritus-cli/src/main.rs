//! Process entry point for the `peritus` command-line client.

fn main() -> std::process::ExitCode {
    peritus_cli::run_env()
}
