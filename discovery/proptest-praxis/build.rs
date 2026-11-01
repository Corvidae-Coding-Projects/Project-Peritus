//! Embed the actual compiler and source fingerprints, and invalidate stale consumer builds.
#[path = "src/fingerprint.rs"]
mod fingerprint;

use std::{env, fs, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("manifest missing")?);
    for path in [
        "src",
        "owner",
        "support",
        "build.rs",
        "Cargo.toml",
        "Cargo.lock",
        "../../crates",
        "../../Cargo.toml",
        "../../Cargo.lock",
        "../../rust-toolchain.toml",
        "../../.cargo/config.toml",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let mut stamp = fingerprint::snapshot(&manifest).map_err(std::io::Error::other)?;
    let rustc = Command::new(env::var_os("RUSTC").ok_or("compiler missing")?)
        .args(["--version", "--verbose"])
        .output()?;
    if !rustc.status.success() {
        return Err("cannot identify actual compiler".into());
    }
    stamp["rustc_verbose"] = String::from_utf8(rustc.stdout)?.trim().into();
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("build output missing")?)
        .join("build-inputs.json");
    fs::write(output, serde_json::to_vec_pretty(&stamp)?)?;
    Ok(())
}
