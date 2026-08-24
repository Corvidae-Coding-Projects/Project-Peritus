//! Installed direct-child helper for the Linux native sandbox backend.

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = peritus_sandbox_linux::run_linux_helper() {
        use std::io::Write as _;

        let message = error.to_string();
        let _ = std::io::stderr()
            .write_all(message.as_bytes())
            .and_then(|()| std::io::stderr().write_all(b"\n"));
        std::process::exit(peritus_sandbox_linux::helper_exit_code(&error));
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    use std::io::Write as _;

    let _ = std::io::stderr()
        .write_all(b"peritus Linux sandbox helper is unavailable on this target\n");
    std::process::exit(122);
}
