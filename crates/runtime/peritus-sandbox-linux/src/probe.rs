//! Runtime Linux capability probing.

mod model;

pub use model::{Architecture, BubblewrapProbe, KernelVersion, NamespaceSupport, ProbeRequest};

use crate::LinuxError;
#[cfg(target_os = "linux")]
use crate::{LinuxErrorKind, LinuxOperation, LinuxRecovery, ProxyRoute};
use peritus_types::Sha256Digest;
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::{
    fs::{self, File},
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

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
        platform_probe(request)
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
fn platform_probe(request: &ProbeRequest) -> Result<LinuxProbe, LinuxError> {
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .and_then(|release| KernelVersion::parse(release.trim()).ok());
    let architecture = Architecture::current();
    let user_enabled = fs::read_to_string("/proc/sys/user/max_user_namespaces")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .is_some_and(|value| value > 0);
    let bubblewrap = probe_bubblewrap(&request.bubblewrap_path);
    let namespaces = NamespaceSupport {
        user: user_enabled && Path::new("/proc/self/ns/user").exists(),
        mount: Path::new("/proc/self/ns/mnt").exists(),
        pid: Path::new("/proc/self/ns/pid").exists(),
        ipc: Path::new("/proc/self/ns/ipc").exists(),
        uts: Path::new("/proc/self/ns/uts").exists(),
        network: Path::new("/proc/self/ns/net").exists(),
        functional: bubblewrap.functional,
    };
    let helper_digest = hash_file(&request.helper_path).ok();
    let landlock_abi = probe_landlock(&request.helper_path);
    let seccomp = probe_seccomp(&request.helper_path);
    let cgroup = crate::CgroupSupport::probe(&request.cgroup_root);
    let pty = File::options().read(true).write(true).open("/dev/ptmx").is_ok();
    let proxy_reachable = request.proxy_route.is_some_and(|route| {
        probe_proxy_in_namespace(&request.bubblewrap_path, &request.helper_path, route)
    });
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
fn platform_probe(request: &ProbeRequest) -> Result<LinuxProbe, LinuxError> {
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
        cgroup.delegated(),
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
fn probe_bubblewrap(path: &Path) -> BubblewrapProbe {
    let version = run_bounded(path, ["--version"])
        .filter(|output| output.status.success())
        .and_then(|output| bounded_output(&output.stdout));
    let functional = run_bounded(
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
    )
    .is_some_and(|output| output.status.success());
    BubblewrapProbe {
        path: path.to_path_buf(),
        version,
        executable_digest: hash_file(path).ok(),
        functional,
    }
}

#[cfg(target_os = "linux")]
fn probe_landlock(helper: &Path) -> Option<u8> {
    let output = run_bounded(helper, ["--probe-landlock"])?;
    if !output.status.success() {
        return None;
    }
    core::str::from_utf8(&output.stdout).ok()?.trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn probe_seccomp(helper: &Path) -> bool {
    run_bounded(helper, ["--probe-seccomp"]).is_some_and(|output| output.status.success())
}

#[cfg(target_os = "linux")]
fn probe_proxy_in_namespace(bwrap: &Path, helper: &Path, route: ProxyRoute) -> bool {
    let endpoint = route.endpoint().to_string();
    let helper = helper.to_string_lossy().into_owned();
    run_bounded(
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
    )
    .is_some_and(|output| output.status.success())
}

#[cfg(target_os = "linux")]
fn run_bounded<const N: usize>(program: &Path, args: [&str; N]) -> Option<std::process::Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(_) => return child.wait_with_output().ok(),
            None if started.elapsed() < PROBE_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10));
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
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
fn hash_file(path: &Path) -> Result<Sha256Digest, LinuxError> {
    let mut file = File::open(path)
        .map_err(|error| LinuxError::io(LinuxOperation::Probe, "open executable", &error))?;
    let metadata = file
        .metadata()
        .map_err(|error| LinuxError::io(LinuxOperation::Probe, "inspect executable", &error))?;
    if metadata.len() > 128 * 1024 * 1024 {
        return Err(probe_error("executable exceeds identity hashing bound"));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.read_to_end(&mut bytes)
        .map_err(|error| LinuxError::io(LinuxOperation::Probe, "hash executable", &error))?;
    Ok(peritus_codec::sha256(&bytes))
}

#[cfg(target_os = "linux")]
fn probe_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::ProbeFailed,
        LinuxOperation::Probe,
        LinuxRecovery::ConfigureHost,
        detail,
    )
}
