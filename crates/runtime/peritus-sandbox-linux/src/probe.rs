//! Runtime Linux capability probing.

mod model;

pub use model::{Architecture, BubblewrapProbe, KernelVersion, NamespaceSupport, ProbeRequest};

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
#[cfg(target_os = "linux")]
use crate::ProxyRoute;
use peritus_types::Sha256Digest;
#[cfg(target_os = "linux")]
use sha2::{Digest as _, Sha256};
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::{
    fs::{self, File},
    io::Read,
    process::{Command, ExitStatus, Stdio},
};

/// Truthful bounded runtime capability result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxProbe {
    kernel: Option<KernelVersion>,
    architecture: Architecture,
    namespaces: NamespaceSupport,
    bubblewrap: BubblewrapProbe,
    helper_digest: Option<Sha256Digest>,
    landlock_abi: Option<u8>,
    seccomp: bool,
    cgroup: crate::CgroupSupport,
    pty: bool,
    proxy_reachable: bool,
    canonical_bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl LinuxProbe {
    /// Executes all configured runtime probes. Individual missing facilities are represented as
    /// false/absent facts; only failure to construct a bounded observation is an error.
    ///
    /// # Errors
    /// Returns `ProbeFailed` when host facts cannot be encoded safely.
    pub fn run(request: &ProbeRequest) -> Result<Self, LinuxError> {
        Self::run_cancellable(request, || true)
    }

    /// Executes the host probe while the caller retains cancellation ownership.
    ///
    /// # Errors
    /// Returns `ProbeFailed` when the caller cancels or host facts cannot be encoded safely.
    pub fn run_cancellable(
        request: &ProbeRequest,
        mut should_continue: impl FnMut() -> bool,
    ) -> Result<Self, LinuxError> {
        platform_probe(request, &mut should_continue)
    }
    /// Returns the parsed kernel version.
    #[must_use]
    pub const fn kernel(&self) -> Option<KernelVersion> {
        self.kernel
    }
    /// Returns the architecture.
    #[must_use]
    pub const fn architecture(&self) -> &Architecture {
        &self.architecture
    }
    /// Returns namespace facts.
    #[must_use]
    pub const fn namespaces(&self) -> NamespaceSupport {
        self.namespaces
    }
    /// Returns bubblewrap facts.
    #[must_use]
    pub const fn bubblewrap(&self) -> &BubblewrapProbe {
        &self.bubblewrap
    }
    /// Returns the exact helper executable digest.
    #[must_use]
    pub const fn helper_digest(&self) -> Option<Sha256Digest> {
        self.helper_digest
    }
    /// Returns the probed Landlock ABI.
    #[must_use]
    pub const fn landlock_abi(&self) -> Option<u8> {
        self.landlock_abi
    }
    /// Reports seccomp-BPF availability.
    #[must_use]
    pub const fn seccomp(&self) -> bool {
        self.seccomp
    }
    /// Returns cgroup-v2 delegation facts.
    #[must_use]
    pub const fn cgroup(&self) -> &crate::CgroupSupport {
        &self.cgroup
    }
    /// Reports PTY availability.
    #[must_use]
    pub const fn pty(&self) -> bool {
        self.pty
    }
    /// Reports whether the configured proxy is reachable inside the new network namespace.
    #[must_use]
    pub const fn proxy_reachable(&self) -> bool {
        self.proxy_reachable
    }
    /// Returns complete canonical probe bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Returns the probe digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Reports baseline R-C3-005 support, excluding plan-specific proxy and cgroup requirements.
    #[must_use]
    pub fn baseline_supported(&self) -> bool {
        self.kernel.is_some_and(|version| version >= crate::MINIMUM_KERNEL)
            && self.architecture.supported()
            && self.namespaces.complete()
            && self.bubblewrap.functional
            && self.helper_digest.is_some()
            && self.landlock_abi.is_some_and(|abi| abi >= crate::MINIMUM_LANDLOCK_ABI)
            && self.seccomp
            && self.pty
    }
}

#[cfg(target_os = "linux")]
fn platform_probe(
    request: &ProbeRequest,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<LinuxProbe, LinuxError> {
    ensure_probe_continues(should_continue)?;
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .and_then(|release| KernelVersion::parse(release.trim()).ok());
    let architecture = Architecture::current();
    let user_enabled = fs::read_to_string("/proc/sys/user/max_user_namespaces")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .is_some_and(|value| value > 0);
    let bubblewrap = probe_bubblewrap(&request.bubblewrap_path, should_continue)?;
    let namespaces = NamespaceSupport {
        user: user_enabled && Path::new("/proc/self/ns/user").exists(),
        mount: Path::new("/proc/self/ns/mnt").exists(),
        pid: Path::new("/proc/self/ns/pid").exists(),
        ipc: Path::new("/proc/self/ns/ipc").exists(),
        uts: Path::new("/proc/self/ns/uts").exists(),
        network: Path::new("/proc/self/ns/net").exists(),
        functional: bubblewrap.functional,
    };
    let helper_digest = hash_file(&request.helper_path, should_continue)?;
    let landlock_abi = probe_landlock(&request.helper_path, should_continue)?;
    let seccomp = probe_seccomp(&request.helper_path, should_continue)?;
    let cgroup = crate::CgroupSupport::probe(&request.cgroup_root);
    let pty = File::options().read(true).write(true).open("/dev/ptmx").is_ok();
    let proxy_reachable = if let Some(route) = request.proxy_route {
        probe_proxy_in_namespace(
            &request.bubblewrap_path,
            &request.helper_path,
            route,
            should_continue,
        )?
    } else {
        false
    };
    ensure_probe_continues(should_continue)?;
    finish_probe(
        kernel,
        architecture,
        namespaces,
        bubblewrap,
        helper_digest,
        landlock_abi,
        seccomp,
        cgroup,
        pty,
        proxy_reachable,
    )
}

#[cfg(not(target_os = "linux"))]
fn platform_probe(
    request: &ProbeRequest,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<LinuxProbe, LinuxError> {
    ensure_probe_continues(should_continue)?;
    finish_probe(
        None,
        Architecture::current(),
        NamespaceSupport::default(),
        BubblewrapProbe {
            path: request.bubblewrap_path.clone(),
            version: None,
            executable_digest: None,
            functional: false,
        },
        None,
        None,
        false,
        crate::CgroupSupport::unavailable(request.cgroup_root.clone()),
        false,
        false,
    )
}

#[allow(clippy::too_many_arguments, reason = "one value per independently probed facility")]
fn finish_probe(
    kernel: Option<KernelVersion>,
    architecture: Architecture,
    namespaces: NamespaceSupport,
    bubblewrap: BubblewrapProbe,
    helper_digest: Option<Sha256Digest>,
    landlock_abi: Option<u8>,
    seccomp: bool,
    cgroup: crate::CgroupSupport,
    pty: bool,
    proxy_reachable: bool,
) -> Result<LinuxProbe, LinuxError> {
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"peritus.linux.probe\0");
    for value in kernel.map_or([0_u16; 3], |v| [v.major, v.minor, v.patch]) {
        canonical.extend_from_slice(&value.to_be_bytes());
    }
    crate::canonical::push_str(&mut canonical, &format!("{architecture:?}"))?;
    for fact in [
        namespaces.user,
        namespaces.mount,
        namespaces.pid,
        namespaces.ipc,
        namespaces.uts,
        namespaces.network,
        namespaces.functional,
        bubblewrap.functional,
        seccomp,
        cgroup.writable_containment(),
        cgroup.controller_delegated("cpu"),
        cgroup.controller_delegated("memory"),
        cgroup.controller_delegated("pids"),
        pty,
        proxy_reachable,
    ] {
        canonical.push(u8::from(fact));
    }
    crate::canonical::push_str(&mut canonical, bubblewrap.path.to_string_lossy().as_ref())?;
    crate::canonical::push_str(&mut canonical, bubblewrap.version.as_deref().unwrap_or(""))?;
    canonical.extend_from_slice(
        bubblewrap.executable_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes(),
    );
    canonical.extend_from_slice(helper_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    canonical.push(landlock_abi.unwrap_or(0));
    let digest = peritus_codec::sha256(&canonical);
    Ok(LinuxProbe {
        kernel,
        architecture,
        namespaces,
        bubblewrap,
        helper_digest,
        landlock_abi,
        seccomp,
        cgroup,
        pty,
        proxy_reachable,
        canonical_bytes: canonical,
        digest,
    })
}

#[cfg(target_os = "linux")]
fn probe_bubblewrap(
    path: &Path,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<BubblewrapProbe, LinuxError> {
    let version = run_probe(path, ["--version"], true, should_continue)?
        .filter(|output| output.status.success())
        .and_then(|output| bounded_output(&output.stdout));
    let functional = run_probe(
        path,
        [
            "--die-with-parent",
            "--new-session",
            "--unshare-user",
            "--unshare-pid",
            "--unshare-ipc",
            "--unshare-uts",
            "--unshare-net",
            "--clearenv",
            "--ro-bind",
            "/",
            "/",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--",
            "/bin/true",
        ],
        false,
        should_continue,
    )?
    .is_some_and(|output| output.status.success());
    Ok(BubblewrapProbe {
        path: path.to_path_buf(),
        version,
        executable_digest: hash_file(path, should_continue)?,
        functional,
    })
}

#[cfg(target_os = "linux")]
fn probe_landlock(
    helper: &Path,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<u8>, LinuxError> {
    let Some(output) = run_probe(helper, ["--probe-landlock"], true, should_continue)? else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    Ok(core::str::from_utf8(&output.stdout).ok().and_then(|value| value.trim().parse().ok()))
}

#[cfg(target_os = "linux")]
fn probe_seccomp(
    helper: &Path,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<bool, LinuxError> {
    Ok(run_probe(helper, ["--probe-seccomp"], false, should_continue)?
        .is_some_and(|output| output.status.success()))
}

#[cfg(target_os = "linux")]
fn probe_proxy_in_namespace(
    bwrap: &Path,
    helper: &Path,
    route: ProxyRoute,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<bool, LinuxError> {
    let endpoint = route.endpoint().to_string();
    let helper = helper.to_string_lossy().into_owned();
    Ok(run_probe(
        bwrap,
        [
            "--die-with-parent",
            "--unshare-user",
            "--unshare-net",
            "--ro-bind",
            "/",
            "/",
            "--",
            helper.as_str(),
            "--probe-proxy",
            endpoint.as_str(),
        ],
        false,
        should_continue,
    )?
    .is_some_and(|output| output.status.success()))
}

#[cfg(target_os = "linux")]
struct ProbeOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
}

#[cfg(target_os = "linux")]
fn run_probe<const N: usize>(
    program: &Path,
    args: [&str; N],
    capture_stdout: bool,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<ProbeOutput>, LinuxError> {
    ensure_probe_continues(should_continue)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(if capture_stdout { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return Ok(None),
    };
    let output = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            let mut retained = Vec::with_capacity(257);
            let mut buffer = [0_u8; 8 * 1_024];
            loop {
                let count = stdout.read(&mut buffer)?;
                if count == 0 {
                    return Ok::<_, std::io::Error>(retained);
                }
                let available = 257_usize.saturating_sub(retained.len()).min(count);
                retained.extend_from_slice(&buffer[..available]);
            }
        })
    });
    loop {
        if !should_continue() {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(output) = output {
                let _ = output.join();
            }
            return Err(probe_cancelled());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = match output {
                    Some(output) => output.join().ok().and_then(Result::ok).unwrap_or_default(),
                    None => Vec::new(),
                };
                return Ok(Some(ProbeOutput { status, stdout }));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(2)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(output) = output {
                    let _ = output.join();
                }
                return Ok(None);
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn bounded_output(bytes: &[u8]) -> Option<String> {
    if bytes.len() > 256 {
        return None;
    }
    let value = core::str::from_utf8(bytes).ok()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(target_os = "linux")]
fn hash_file(
    path: &Path,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<Sha256Digest>, LinuxError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        ensure_probe_continues(should_continue)?;
        let count = match file.read(&mut buffer) {
            Ok(count) => count,
            Err(_) => return Ok(None),
        };
        if count == 0 {
            return Ok(Some(Sha256Digest::new(digest.finalize().into())));
        }
        digest.update(&buffer[..count]);
    }
}

fn ensure_probe_continues(
    should_continue: &mut impl FnMut() -> bool,
) -> Result<(), LinuxError> {
    if should_continue() { Ok(()) } else { Err(probe_cancelled()) }
}

fn probe_cancelled() -> LinuxError {
    probe_error("native capability probe was cancelled by its caller")
}

fn probe_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::ProbeFailed,
        LinuxOperation::Probe,
        LinuxRecovery::ConfigureHost,
        detail,
    )
}
