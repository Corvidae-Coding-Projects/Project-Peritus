//! Process entry point for the `peritus` command-line client.

fn main() -> std::process::ExitCode {
    if peritus_tools_computer::native_host_requested() {
        return match peritus_tools_computer::run_chrome_native_host() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("peritus-browser-host: {error}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    peritus_cli::run_env()
}
