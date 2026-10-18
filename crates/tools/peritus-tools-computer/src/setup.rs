//! Per-user browser runtime provisioning and readiness publication.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::{Value, json};

use super::{browser_cache, installation_root, require_success};

mod bootstrap;

const RUNTIME: &str = "runtimes/playwright-1.64.0-pillow-13.0.0";
const PYTHON: &str = if cfg!(windows) { "Scripts/python.exe" } else { "bin/python" };

fn ready(root: &Path) -> Option<Value> {
    let value: Value = serde_json::from_slice(&fs::read(root.join("runtime.json")).ok()?).ok()?;
    (value["schema"] == 1 && value["runtime"] == RUNTIME).then_some(value)
}

pub(super) fn selected_python(root: &Path) -> Result<Option<PathBuf>, String> {
    Ok(ready(root).and_then(|_| {
        let path = root.join(RUNTIME).join(PYTHON);
        path.is_file().then_some(path)
    }))
}

/// Provisions a missing runtime for portable/system-package first launches.
/// Native installers call [`setup`] before publishing a successful installation.
pub fn ensure_setup() -> Result<(), String> {
    let root = installation_root()?;
    if env::var_os("PERITUS_AUTOMATION_PYTHON").is_some() {
        // An explicit runtime is owned by its operator, not this installer.
        check_browser(&super::python()?, &browser_cache()?)?;
        return Ok(());
    }
    if let Some(value) = ready(&root)
        && root.join(RUNTIME).join(PYTHON).is_file()
        && value["browser_executable"].as_str().is_some_and(|p| Path::new(p).is_file())
    {
        return Ok(());
    }
    setup().map(|_| ())
}

/// Installs and checks Python, Playwright, Pillow and Chromium in per-user storage.
/// Downloads are pinned, concurrent installations serialize, and readiness is published last.
pub fn setup() -> Result<Value, String> {
    // Keep the synchronous API usable from callers which already own an async runtime.
    std::thread::spawn(setup_inner)
        .join()
        .map_err(|_| "automation setup worker failed".to_owned())?
}

fn setup_inner() -> Result<Value, String> {
    let root = installation_root()?;
    fs::create_dir_all(&root).map_err(|e| format!("create automation directory: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("setup.lock"))
        .map_err(|e| format!("open automation setup lock: {e}"))?;
    lock.lock().map_err(|e| format!("lock automation setup: {e}"))?;
    let cache = browser_cache()?;
    if env::var_os("PERITUS_AUTOMATION_PYTHON").is_some() {
        let python = super::python()?;
        check_browser(&python, &cache).map_err(|e| format!("The explicit PERITUS_AUTOMATION_PYTHON runtime is not ready: {e}. Unset the override to use automatic managed installation."))?;
        return super::status();
    }
    if env::var_os("PERITUS_INSTALL_DEPS").is_some_and(|v| v == "0") {
        if let Some(value) = ready(&root)
            && root.join(RUNTIME).join(PYTHON).is_file()
            && value["browser_executable"].as_str().is_some_and(|p| Path::new(p).is_file())
        {
            check_browser(&root.join(RUNTIME).join(PYTHON), &cache)?;
            return super::status();
        }
        return Err("Browser automation is not installed and PERITUS_INSTALL_DEPS=0 disables automatic dependency installation".into());
    }
    eprintln!("Preparing Peritus browser automation (Python, Playwright and Chromium)…");
    let uv = bootstrap::install(&root)?;
    let runtime = root.join(RUNTIME);
    let python = runtime.join(PYTHON);
    if !compatible_python(&python) {
        let base = super::python().ok().filter(|python| compatible_python(python));
        let mut command = uv_command(&uv, &root);
        // Only this versioned managed environment is replaceable. Legacy runtimes stay intact.
        // This also repairs an interrupted setup which created an incompatible environment.
        command.args(["venv", "--clear", "--no-project", "--system-site-packages", "--python"]);
        if let Some(base) = base {
            command.arg(base);
        } else {
            command.args(["3.13", "--managed-python"]);
        }
        command.arg(&runtime);
        require_success(&mut command, "prepare automation Python")?;
    }
    require_success(
        uv_command(&uv, &root)
            .args(["pip", "install", "--python"])
            .arg(&python)
            .args(["playwright==1.64.0", "Pillow==13.0.0"]),
        "install automation libraries",
    )?;
    require_success(
        Command::new(&python).env("PLAYWRIGHT_BROWSERS_PATH", &cache).args([
            "-m",
            "playwright",
            "install",
            "chromium",
        ]),
        "install automation Chromium",
    )?;
    let checked = match check_browser(&python, &cache) {
        Ok(checked) => checked,
        Err(error) if cfg!(target_os = "linux") && missing_libraries(&error) => {
            install_linux_libraries(&python)?;
            check_browser(&python, &cache)?
        }
        Err(error) => return Err(error),
    };
    let receipt = json!({"schema":1,"runtime":RUNTIME,"browser_executable":checked["browser_executable"],"browser_version":checked["browser_version"]});
    let mut temporary = tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
    use std::io::Write;
    temporary
        .write_all(serde_json::to_string(&receipt).map_err(|e| e.to_string())?.as_bytes())
        .map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary
        .persist(root.join("runtime.json"))
        .map_err(|e| format!("publish automation readiness: {e}"))?;
    let mut status = super::status()?;
    status["browser_ready"] = json!(true);
    status["browser_version"] = checked["browser_version"].clone();
    Ok(status)
}

fn compatible_python(python: &Path) -> bool {
    // Pillow 13 requires Python >=3.11; prefer the checked versions or managed 3.13.
    Command::new(python)
        .args(["-c", "import sys; sys.exit(not ((3,11) <= sys.version_info[:2] < (3,15)))"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn uv_command(uv: &Path, root: &Path) -> Command {
    let mut command = Command::new(uv);
    command
        .arg("--no-config")
        .env("UV_PYTHON_INSTALL_DIR", root.join("python"))
        .env("UV_CACHE_DIR", root.join("cache"))
        .env("UV_PYTHON_DOWNLOADS", "automatic")
        .env("UV_PYTHON_INSTALL_BIN", "0")
        .env("UV_PYTHON_INSTALL_REGISTRY", "0")
        .env_remove("VIRTUAL_ENV");
    command
}

fn check_browser(python: &Path, cache: &Path) -> Result<Value, String> {
    let output = Command::new(python).env("PLAYWRIGHT_BROWSERS_PATH", cache)
        .args(["-c", r#"import json
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    with p.chromium.launch(headless=True, chromium_sandbox=True) as browser:
        page = browser.new_page()
        if page.evaluate('1 + 1') != 2:
            raise RuntimeError('Chromium did not execute JavaScript correctly')
        print(json.dumps({'browser_executable': p.chromium.executable_path, 'browser_version': browser.version}))
"#]).output().map_err(|e| format!("check automation browser: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "automation browser could not launch: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("read automation browser readiness: {e}"))
}

fn missing_libraries(error: &str) -> bool {
    error.contains("Host system is missing dependencies")
        || error.contains("error while loading shared libraries")
        || error.contains("Missing libraries:")
}

fn install_linux_libraries(python: &Path) -> Result<(), String> {
    if env::var_os("PERITUS_INSTALL_DEPS").is_some_and(|v| v == "0") {
        return Err("Chromium system libraries are missing and PERITUS_INSTALL_DEPS=0 disables dependency installation".into());
    }
    let have = |name: &str| {
        env::split_paths(&env::var_os("PATH").unwrap_or_default()).any(|p| p.join(name).is_file())
    };
    if have("apt-get") {
        return require_success(
            Command::new(python).args(["-m", "playwright", "install-deps", "chromium"]),
            "install Chromium system libraries",
        );
    }
    let (program, arguments): (&str, &[&str]) = if have("dnf") {
        (
            "dnf",
            &[
                "install",
                "--assumeyes",
                "alsa-lib",
                "at-spi2-atk",
                "at-spi2-core",
                "cairo",
                "cups-libs",
                "gtk3",
                "libdrm",
                "libX11",
                "libxcb",
                "libXcomposite",
                "libXdamage",
                "libXext",
                "libXfixes",
                "libXrandr",
                "libxkbcommon",
                "mesa-libgbm",
                "nspr",
                "nss",
                "pango",
            ],
        )
    } else if have("pacman") {
        (
            "pacman",
            &[
                "-S",
                "--needed",
                "--noconfirm",
                "alsa-lib",
                "at-spi2-core",
                "cairo",
                "cups",
                "gtk3",
                "libdrm",
                "libx11",
                "libxcb",
                "libxcomposite",
                "libxdamage",
                "libxext",
                "libxfixes",
                "libxrandr",
                "libxkbcommon",
                "mesa",
                "nss",
                "nspr",
                "pango",
            ],
        )
    } else if have("zypper") {
        (
            "zypper",
            &[
                "--non-interactive",
                "install",
                "alsa",
                "libatk-1_0-0",
                "at-spi2-core",
                "libcairo2",
                "libcups2",
                "libgtk-3-0",
                "libdrm2",
                "libX11-6",
                "libxcb1",
                "libXcomposite1",
                "libXdamage1",
                "libXext6",
                "libXfixes3",
                "libXrandr2",
                "libxkbcommon0",
                "libgbm1",
                "mozilla-nss",
                "mozilla-nspr",
                "libpango-1_0-0",
            ],
        )
    } else {
        return Err("Chromium system libraries are missing; no supported Linux package manager is available".into());
    };
    let root = Command::new("id").arg("-u").output().map_err(|e| e.to_string())?.stdout == b"0\n";
    let mut command = if root {
        Command::new(program)
    } else {
        let mut command = Command::new("sudo");
        command.args(["--", program]);
        command
    };
    require_success(command.args(arguments), "install Chromium system libraries")
}
