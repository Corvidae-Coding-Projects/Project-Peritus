//! Bounded local RPC and setup for ordinary computer-control adapters.

use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{IpAddr, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde_json::{Value, json};

mod setup;
pub use setup::{ensure_setup, setup};

/// Maximum controller observation size, excluding separately retained image bytes.
pub const MAX_RESPONSE_BYTES: u64 = 256 * 1024;
/// Maximum accepted request size.
pub const MAX_REQUEST_BYTES: usize = 128 * 1024;

/// Resolves Playwright's user installation cache before the command sandbox changes HOME.
/// This contains browser executables, never personal browsing profiles or credentials.
///
/// # Errors
///
/// Returns an error when the configured cache is relative or no user cache root is available.
pub fn browser_cache() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        let path = PathBuf::from(path);
        if path == Path::new("0") || path.is_absolute() {
            return Ok(path);
        }
        return Err("PLAYWRIGHT_BROWSERS_PATH must be absolute or 0".into());
    }
    let root = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Caches"))
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
    };
    root.map(|p| p.join("ms-playwright"))
        .ok_or_else(|| "browser installation cache is unavailable".into())
}

/// Optional per-user automation installation root.
///
/// # Errors
///
/// Returns an error when no per-user application data directory can be resolved.
pub fn installation_root() -> Result<PathBuf, String> {
    let root = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("Peritus"))
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME")
            .map(|path| PathBuf::from(path).join("Library/Application Support/Peritus"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/share")))
            .map(|path| path.join("peritus"))
    };
    root.map(|path| path.join("automation"))
        .ok_or_else(|| "user data directory is unavailable".into())
}

/// Resolves the explicitly selected or installed Python runtime.
///
/// # Errors
///
/// Returns an error when an override is invalid or no usable Python executable is available.
pub fn python() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("PERITUS_AUTOMATION_PYTHON") {
        let path = PathBuf::from(path);
        if !path.is_absolute() || !path.is_file() {
            return Err("PERITUS_AUTOMATION_PYTHON must name an absolute Python executable".into());
        }
        return Ok(path);
    }
    let root = installation_root()?;
    if let Some(python) = setup::selected_python(&root) {
        return Ok(python);
    }
    let installed =
        root.join(if cfg!(windows) { "runtime/Scripts/python.exe" } else { "runtime/bin/python" });
    if installed.is_file() {
        return Ok(installed);
    }
    let names: &[&str] = if cfg!(windows) { &["python.exe", "python3.exe"] } else { &["python3"] };
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err("Python 3 is unavailable; install Python and run peritus automation setup".into())
}

/// Publishes the bundled driver below an application-owned runtime directory.
///
/// # Errors
///
/// Returns an error when the destination is unsafe or driver files cannot be published.
pub fn prepare_driver(directory: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    for (name, source) in [
        ("driver.py", include_str!("driver.py")),
        ("browser.py", include_str!("browser.py")),
        ("desktop.py", include_str!("desktop.py")),
    ] {
        let path = directory.join(name);
        if path.is_symlink() {
            return Err("automation driver path is a symbolic link".into());
        }
        fs::write(path, source).map_err(|error| error.to_string())?;
    }
    Ok(directory.join("driver.py"))
}

/// Reports available real runtime dependencies without making provider requests.
///
/// # Errors
///
/// Returns an error when the driver cannot be prepared, launched, or decoded.
pub fn status() -> Result<Value, String> {
    let script = prepare_driver(&installation_root()?.join("driver"))?;
    let output = Command::new(python()?)
        .arg(script)
        .arg("status")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

fn require_success(command: &mut Command, operation: &str) -> Result<(), String> {
    // CLI stdout carries the result JSON; dependency progress belongs on stderr.
    let status = command
        .stdout(std::io::stderr())
        .status()
        .map_err(|error| format!("{operation}: {error}"))?;
    if status.success() { Ok(()) } else { Err(format!("{operation}: {status}")) }
}

/// Sends one authenticated request to an already owned driver.
///
/// # Errors
///
/// Returns an error when readiness is invalid, transport fails, or the response is malformed.
pub fn request(directory: &Path, id: &str, method: &str, params: &Value) -> Result<Value, String> {
    let ready_path = directory.join("ready.json");
    if ready_path.is_symlink() {
        return Err("automation readiness record is a symbolic link".into());
    }
    let ready: Value =
        serde_json::from_slice(&fs::read(ready_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let port = ready["port"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .ok_or("invalid automation port")?;
    let token = ready["token"].as_str().ok_or("missing automation authentication")?;
    let mut encoded =
        serde_json::to_vec(&json!({"token":token,"id":id,"method":method,"params":params}))
            .map_err(|error| error.to_string())?;
    if encoded.len() > MAX_REQUEST_BYTES {
        return Err("automation request exceeds its byte limit".into());
    }
    encoded.push(b'\n');
    let mut stream = TcpStream::connect_timeout(
        &(IpAddr::from([127, 0, 0, 1]), port).into(),
        Duration::from_secs(5),
    )
    .map_err(|error| format!("automation controller unavailable: {error}; reopen the session"))?;
    stream.set_read_timeout(Some(Duration::from_secs(45))).map_err(|error| error.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|error| error.to_string())?;
    stream.write_all(&encoded).map_err(|error| error.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .take(MAX_RESPONSE_BYTES + 1)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    if line.len() as u64 > MAX_RESPONSE_BYTES || !line.ends_with('\n') {
        return Err(
            "automation response was oversized or interrupted; do not repeat an action blindly"
                .into(),
        );
    }
    serde_json::from_str(&line).map_err(|error| error.to_string())
}
