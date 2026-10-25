//! Exact managed-browser `AppArmor` permissions without disabling Chromium's sandbox.

use std::{fmt::Write as _, fs, io::Write as _, path::Path, process::Command};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::require_success;

pub(super) fn namespace_denied(error: &str) -> bool {
    (error.contains("No usable sandbox!") || error.contains("Chromium sandboxing failed!"))
        && restricted()
}

pub(super) fn restricted() -> bool {
    fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
        .is_ok_and(|value| value.trim() == "1")
}

#[derive(Deserialize)]
struct ManagedBrowsers {
    root: std::path::PathBuf,
    executables: Vec<std::path::PathBuf>,
    uid: u32,
}

pub(super) fn install_browser_policy(
    python: &Path,
    cache: &Path,
    installation: &Path,
) -> Result<(), String> {
    let parser = ["/usr/sbin/apparmor_parser", "/sbin/apparmor_parser"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .ok_or("Chromium needs an AppArmor user-namespace policy; install the AppArmor parser, then retry peritus automation setup")?;
    // Playwright's public executable path names full Chrome, whereas its default
    // headless launch uses another executable. Cover both actual managed programs.
    let output = Command::new(python)
        .env("PLAYWRIGHT_BROWSERS_PATH", cache)
        .args(["-c", r#"import json, os, subprocess
from pathlib import Path
from playwright._impl._driver import compute_driver_executable
node, cli = compute_driver_executable()
registry = Path(cli).parent / 'lib/coreBundle.js'
script = "const {registry,registryDirectory}=require(process.argv[1]).registry; console.log(JSON.stringify({root:registryDirectory,paths:['chromium','chromium-headless-shell'].map(name=>registry.findExecutable(name).executablePath())}))"
selected = json.loads(subprocess.check_output([node, '-e', script, str(registry)], text=True))
root = Path(selected['root']).resolve(strict=True)
executables = []
for value in selected['paths']:
    target = Path(value).resolve(strict=True)
    if not target.is_relative_to(root) or not target.is_file() or target.stat().st_uid != os.getuid() or not os.access(target, os.X_OK):
        raise RuntimeError('Refusing an AppArmor attachment outside this user\'s managed browser executables')
    executables.append(str(target))
print(json.dumps({'root': str(root), 'executables': executables, 'uid': os.getuid()}))
"#])
        .output()
        .map_err(|error| format!("locate managed Chromium executables: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "locate managed Chromium executables: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let browsers: ManagedBrowsers = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("read managed Chromium paths: {error}"))?;
    if !browsers.root.is_absolute() || browsers.executables.len() != 2 {
        return Err("managed Chromium executables were not found for AppArmor setup".into());
    }
    let mut policy = String::from(
        "# Managed by Peritus: exact Chromium user-namespace permissions.\nabi <abi/4.0>,\ninclude <tunables/global>\n\n",
    );
    let mut hasher = Sha256::new();
    hasher.update(browsers.root.as_os_str().as_encoded_bytes());
    hasher.update(browsers.uid.to_le_bytes());
    let mut identity = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(identity, "{byte:02x}");
    }
    for (index, executable) in browsers.executables.iter().enumerate() {
        if !executable.starts_with(&browsers.root) || !executable.is_file() {
            return Err("refusing an AppArmor attachment outside the managed browser cache".into());
        }
        let path = executable.to_str().ok_or("AppArmor browser paths must be valid UTF-8")?;
        let quoted = literal_path(path)?;
        let _ = writeln!(
            policy,
            "profile peritus-chromium-{identity}-{index} \"{quoted}\" flags=(unconfined) {{\n  userns,\n}}\n"
        );
    }
    let destination = format!("/etc/apparmor.d/peritus-chromium-{identity}");
    let mut source = tempfile::NamedTempFile::new_in(installation)
        .map_err(|error| format!("prepare Chromium AppArmor policy: {error}"))?;
    source.write_all(policy.as_bytes()).map_err(|error| error.to_string())?;
    source.flush().map_err(|error| error.to_string())?;
    require_success(
        Command::new(parser).args(["--skip-kernel-load", "--skip-cache"]).arg(source.path()),
        "parse managed Chromium AppArmor policy",
    )?;
    eprintln!(
        "Installing AppArmor permissions for Peritus's managed Chromium executables (administrator authorization may be required)…"
    );
    require_success(
        privileged("/usr/bin/install")?
            .args(["-o", "root", "-g", "root", "-m", "0644", "--"])
            .arg(source.path())
            .arg(&destination),
        "install managed Chromium AppArmor policy",
    )?;
    require_success(
        privileged(parser)?.arg("--replace").arg(&destination),
        "load managed Chromium AppArmor policy",
    )
}

fn privileged(program: &str) -> Result<Command, String> {
    use std::os::unix::fs::MetadataExt as _;
    let root = fs::metadata("/proc/self").map_err(|error| error.to_string())?.uid() == 0;
    if root {
        return Ok(Command::new(program));
    }
    if !Path::new("/usr/bin/sudo").is_file() {
        return Err("administrator authorization is required for Chromium's AppArmor profile; install sudo or ask your administrator".into());
    }
    let mut command = Command::new("/usr/bin/sudo");
    command.args(["--", program]);
    Ok(command)
}

fn literal_path(path: &str) -> Result<String, String> {
    let mut literal = String::new();
    for character in path.chars() {
        if character.is_control() {
            return Err("Chromium's AppArmor attachment cannot contain control characters".into());
        }
        if matches!(character, '\\' | '"' | '*' | '?' | '[' | ']' | '{' | '}' | '^' | ',') {
            literal.push('\\');
        }
        literal.push(character);
    }
    Ok(literal)
}
