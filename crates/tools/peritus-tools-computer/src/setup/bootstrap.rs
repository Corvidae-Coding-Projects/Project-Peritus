//! Pinned, checksum-verified uv bootstrap; it does not modify PATH or install global Python.

use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const VERSION: &str = "0.12.22";

pub(super) fn install(root: &Path) -> Result<PathBuf, String> {
    let name = if cfg!(windows) { "uv.exe" } else { "uv" };
    let target = root.join("bootstrap").join(VERSION).join(name);
    if target.is_file()
        && Command::new(&target).arg("--version").output().is_ok_and(|v| {
            v.status.success()
                && String::from_utf8_lossy(&v.stdout).starts_with(&format!("uv {VERSION} "))
        })
    {
        return Ok(target);
    }
    let (triple, checksum) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => (
            "x86_64-unknown-linux-musl",
            "a50fd68c653b0cfb1c85e0a7db62cb78cf5c22b6f3dcf3ae173e5f222d084470",
        ),
        ("linux", "aarch64") => (
            "aarch64-unknown-linux-musl",
            "228bd32c180421a94eef91378a92b2dd63c768bd278430e833250524c4a13382",
        ),
        ("macos", "x86_64") => (
            "x86_64-apple-darwin",
            "1b8a5b316883df2daf20fb9a446e5b230e01d947d57aba2694977c5ac5a7e98c",
        ),
        ("macos", "aarch64") => (
            "aarch64-apple-darwin",
            "5d714de09501a59393ceca78f4bc232a50478729640d251907160299b2a93ddd",
        ),
        ("windows", "x86_64") => (
            "x86_64-pc-windows-msvc",
            "ea1397797a0ca15f63516dd0f49c2dde9776db9be5861cab152ebe8ad199894d",
        ),
        ("windows", "aarch64") => (
            "aarch64-pc-windows-msvc",
            "6a42b919c2bb7135f07b4d1bb8e489f0eaab0bae039020573cc522d831e2d32e",
        ),
        _ => return Err(
            "automatic automation installation supports Linux, macOS and Windows on x86-64/ARM64"
                .into(),
        ),
    };
    let temporary = tempfile::tempdir_in(root).map_err(|e| e.to_string())?;
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive = temporary.path().join(format!("uv.{extension}"));
    let url = format!(
        "https://github.com/astral-sh/uv/releases/download/{VERSION}/uv-{triple}.{extension}"
    );
    eprintln!("Downloading Peritus's private Python installer…");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(download(&url, &archive, checksum))?;
    let extracted = temporary.path().join("extracted");
    fs::create_dir(&extracted).map_err(|e| e.to_string())?;
    let status = if cfg!(windows) {
        Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath $env:PERITUS_UV_ARCHIVE -DestinationPath $env:PERITUS_UV_OUTPUT"])
            .env("PERITUS_UV_ARCHIVE", &archive).env("PERITUS_UV_OUTPUT", &extracted).status()
    } else {
        Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(&extracted).status()
    }.map_err(|e| format!("extract Python installer: {e}"))?;
    if !status.success() {
        return Err(format!("extract Python installer: {status}"));
    }
    let nested = extracted.join(format!("uv-{triple}")).join(name);
    let source = if nested.is_file() { nested } else { extracted.join(name) };
    if !fs::symlink_metadata(&source)
        .map_err(|e| format!("find Python installer executable: {e}"))?
        .file_type()
        .is_file()
    {
        return Err("Python installer executable is not a regular file".into());
    }
    let parent = target.parent().ok_or("Python installer has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let staged = parent.join(format!("{name}.new"));
    fs::copy(source, &staged).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    if target.exists() {
        fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    fs::rename(&staged, &target).map_err(|e| format!("publish Python installer: {e}"))?;
    Ok(target)
}

async fn download(url: &str, target: &Path, expected: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(240))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "peritus-automation-setup")
        .send()
        .await
        .map_err(|e| format!("download Python installer: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download Python installer: {e}"))?;
    let mut file = fs::File::create(target).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        bytes += chunk.len() as u64;
        if bytes > 64 * 1024 * 1024 {
            return Err("Python installer archive exceeds 64 MiB".into());
        }
        digest.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
    }
    let actual = digest.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
    if actual != expected {
        return Err("Python installer checksum mismatch; nothing was executed".into());
    }
    file.sync_all().map_err(|e| e.to_string())
}
