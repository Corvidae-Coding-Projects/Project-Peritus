//! Real-interface replay for the OpenCode runtime selected through WSL interop.
//!
//! The consumer uses the production discovery and catalog entry points and then checks the
//! pinned executable against an independent native-host oracle. It exits non-zero when a
//! Windows program reached through WSL interop is accepted as the supported runtime.

use peritus_provider_core::{CancellationToken, Credential, hosted::HostedService};
use peritus_provider_opencode::{OpenCodeExecutable, discover_models};
use std::path::{Component, Path};

/// Independently classifies a pinned executable this host cannot run natively.
fn foreign_reason(path: &Path) -> Option<&'static str> {
    if std::fs::read(path).is_ok_and(|bytes| bytes.starts_with(b"MZ")) {
        return Some("Windows portable executable");
    }
    let mut components = path.components();
    let root = components.next();
    let mounted = components.next();
    let drive = components.next();
    let windows_drive = matches!(root, Some(Component::RootDir))
        && matches!(mounted, Some(Component::Normal(name)) if name == std::ffi::OsStr::new("mnt"))
        && matches!(drive, Some(Component::Normal(name)) if is_windows_drive(name));
    windows_drive.then_some("WSL Windows-drive mount")
}

fn is_windows_drive(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    let mut characters = name.chars();
    characters.next().is_some_and(|letter| letter.is_ascii_alphabetic())
        && characters.next().is_none()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cancellation = CancellationToken::new();
    let Ok(executable) = OpenCodeExecutable::discover_supported(&cancellation).await else {
        println!("runtime none");
        println!("verdict PASS (no supported runtime was pinned)");
        return;
    };
    let path = executable.as_path().to_owned();
    println!("runtime {}", path.display());
    match foreign_reason(&path) {
        Some(reason) => println!("nativity foreign ({reason})"),
        None => println!("nativity native"),
    }
    // A disposable placeholder selects the service only; no inference is requested.
    let credential = Credential::new(b"sk-peritus-replay-placeholder".to_vec())
        .expect("placeholder credential");
    match discover_models(&executable, HostedService::OpenCodeGo, &credential, &cancellation).await {
        Ok(models) => println!("catalog {} advertised models", models.len()),
        Err(error) => println!("catalog {}: {}", error.code(), error.detail()),
    }
    if let Some(reason) = foreign_reason(&path) {
        eprintln!("FAIL: the pinned runtime is a {reason}, which this host cannot run natively");
        std::process::exit(1);
    }
    println!("verdict PASS");
}
