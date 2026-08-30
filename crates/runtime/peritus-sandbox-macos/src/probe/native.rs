//! Bounded macOS host probe effects.

use std::{
    io::Read,
    net::TcpStream,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use peritus_types::Sha256Digest;

use super::{MacosError, MacosHostProbe, ProbeEvidence, ProbeRequest, ResourceProbe};

const MAX_HELPER_BYTES: u64 = 16 * 1_024 * 1_024;

pub(super) fn run_macos_probe(request: &ProbeRequest) -> Result<MacosHostProbe, MacosError> {
    let os_version = command_output("/usr/bin/sw_vers", &["-productVersion"])
        .and_then(|value| parse_version(value.trim()));
    let architecture = matches!(std::env::consts::ARCH, "x86_64" | "aarch64");
    let helper_executable = executable_file(request.helper_path());
    let helper_digest =
        helper_executable.then(|| bounded_helper_digest(request.helper_path())).flatten();
    let seatbelt = executable_file(request.seatbelt_path());
    let profile_compilation = seatbelt
        && command_succeeds(
            request.seatbelt_path(),
            &[
                "-p",
                "(version 1)(deny default)(allow process-exec (literal \"/usr/bin/true\"))",
                "/usr/bin/true",
            ],
            request.connect_timeout,
        );
    let credential_store = executable_file(Path::new("/usr/bin/security"))
        && command_succeeds(
            Path::new("/usr/bin/security"),
            &["list-keychains", "-d", "user"],
            request.connect_timeout,
        );
    let proxy = request.proxy.is_none_or(|route| {
        TcpStream::connect_timeout(&route.endpoint(), request.connect_timeout).is_ok()
    });
    MacosHostProbe::from_evidence(ProbeEvidence {
        os_version,
        platform: true,
        architecture,
        helper: helper_digest.is_some(),
        seatbelt,
        profile_compilation,
        process_containment: probe_process_containment(request.connect_timeout),
        pty: std::fs::File::open("/dev/ptmx").is_ok(),
        credential_store,
        proxy,
        resources: if crate::resource_monitor::native_controls_available() {
            ResourceProbe::macos_production()
        } else {
            ResourceProbe::unsupported()
        },
        helper_digest,
    })
}

fn executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn bounded_helper_digest(path: &Path) -> Option<Sha256Digest> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_HELPER_BYTES.saturating_add(1)).read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() || u64::try_from(bytes.len()).ok()? > MAX_HELPER_BYTES {
        return None;
    }
    Some(peritus_codec::sha256(&bytes))
}

fn command_output(executable: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 1_024 {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn command_succeeds(executable: &Path, arguments: &[&str], timeout: Duration) -> bool {
    let Ok(mut child) = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    wait_success(&mut child, timeout)
}

fn probe_process_containment(timeout: Duration) -> bool {
    use std::os::unix::process::CommandExt as _;
    let mut command = Command::new("/usr/bin/true");
    command.process_group(0).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    command.spawn().is_ok_and(|mut child| wait_success(&mut child, timeout))
}

fn wait_success(child: &mut std::process::Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Err(_) => return false,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn parse_version(value: &str) -> Option<(u16, u16, u16)> {
    let mut components = value.split('.');
    let major = components.next()?.parse().ok()?;
    let minor = components.next().unwrap_or("0").parse().ok()?;
    let patch = components.next().unwrap_or("0").parse().ok()?;
    if components.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}
