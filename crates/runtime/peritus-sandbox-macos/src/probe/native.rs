//! Bounded macOS host probe effects.

#![allow(
    unsafe_code,
    reason = "inventoried macOS socket, process-group, rlimit, and libproc probe boundary"
)]

use std::{
    io::Read as _,
    net::SocketAddr,
    os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use super::{MacosError, MacosHostProbe, ProbeEvidence, ProbeRequest, ResourceProbe};
use crate::{MacosErrorKind, MacosOperation, MacosErrorSource, RecoveryAction};

const MAX_VERSION_OUTPUT_BYTES: u64 = 1_024;
const POLL_INTERVAL: Duration = Duration::from_millis(2);
const DENY_DEFAULT_PROBE: &str = concat!(
    "(version 1)\n",
    "(deny default)\n",
    "(deny network*)\n",
    "(deny process-fork)\n",
    "(allow process-exec (literal \"/usr/bin/true\"))\n",
    "(allow file-read* (literal \"/usr/bin/true\"))\n",
    "(allow file-read* (subpath \"/usr/lib\"))\n",
    "(allow file-read* (subpath \"/System/Library\"))\n",
);

pub(super) fn run_macos_probe(
    request: &ProbeRequest,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<MacosHostProbe, MacosError> {
    ensure_continues(should_continue, None)?;
    let os_version = command_output(
        Path::new("/usr/bin/sw_vers"),
        &["-productVersion"],
        request.operation_timeout(),
        should_continue,
    )?
    .and_then(|value| parse_version(value.trim()));
    let architecture = matches!(std::env::consts::ARCH, "x86_64" | "aarch64");
    let helper_digest = if executable_file(request.helper_path()) {
        helper_digest(request.helper_path(), request.operation_timeout(), should_continue)?
    } else {
        None
    };
    let seatbelt = executable_file(request.seatbelt_path());
    let profile_compilation = seatbelt
        && command_succeeds(
            request.seatbelt_path(),
            &["-p", DENY_DEFAULT_PROBE, "/usr/bin/true"],
            request.operation_timeout(),
            should_continue,
        )?;
    let credential_store = executable_file(Path::new("/usr/bin/security"))
        && command_succeeds(
            Path::new("/usr/bin/security"),
            &["list-keychains", "-d", "user"],
            request.operation_timeout(),
            should_continue,
        )?;
    let proxy = match request.proxy() {
        Some(route) => probe_proxy(
            route.endpoint(),
            request.operation_timeout(),
            should_continue,
        )?,
        None => false,
    };
    let process_containment =
        probe_process_containment(request.operation_timeout(), should_continue)?;
    ensure_continues(should_continue, None)?;
    MacosHostProbe::from_evidence(ProbeEvidence {
        os_version,
        platform: true,
        architecture,
        helper: helper_digest.is_some(),
        seatbelt,
        profile_compilation,
        process_containment,
        pty: std::fs::File::open("/dev/ptmx").is_ok(),
        credential_store,
        proxy,
        resources: if probe_resource_enforcement() {
            ResourceProbe::macos_production()
        } else {
            ResourceProbe::unsupported()
        },
        helper_digest,
    })
}

fn executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.permissions().mode() & 0o111 != 0
    })
}

fn helper_digest(
    path: &Path,
    timeout: Option<Duration>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<Sha256Digest>, MacosError> {
    let deadline = operation_deadline(timeout)?;
    let mut file = std::fs::File::open(path)
        .map_err(|source| probe_io("installed helper cannot be opened for identity probing", source))?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        ensure_continues(should_continue, deadline)?;
        let count = file
            .read(&mut buffer)
            .map_err(|source| probe_io("installed helper identity cannot be read", source))?;
        if count == 0 {
            return Ok((total != 0).then(|| Sha256Digest::new(digest.finalize().into())));
        }
        total = total.checked_add(u64::try_from(count).unwrap_or(u64::MAX)).ok_or_else(|| {
            probe_failed("installed helper identity length is not representable")
        })?;
        digest.update(&buffer[..count]);
    }
}

fn command_output(
    executable: &Path,
    arguments: &[&str],
    timeout: Option<Duration>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<String>, MacosError> {
    let Some(output) = run_command(executable, arguments, true, timeout, should_continue)? else {
        return Ok(None);
    };
    if !output.status.success()
        || u64::try_from(output.stdout.len()).unwrap_or(u64::MAX) > MAX_VERSION_OUTPUT_BYTES
    {
        return Ok(None);
    }
    Ok(String::from_utf8(output.stdout).ok())
}

fn command_succeeds(
    executable: &Path,
    arguments: &[&str],
    timeout: Option<Duration>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<bool, MacosError> {
    Ok(run_command(executable, arguments, false, timeout, should_continue)?
        .is_some_and(|output| output.status.success()))
}

fn run_command(
    executable: &Path,
    arguments: &[&str],
    capture_stdout: bool,
    timeout: Option<Duration>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<ProbeOutput>, MacosError> {
    use std::os::unix::process::CommandExt as _;

    ensure_continues(should_continue, None)?;
    let deadline = operation_deadline(timeout)?;
    let child = match Command::new(executable)
        .args(arguments)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(if capture_stdout { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(probe_io("native probe subprocess could not be created", source)),
    };
    let mut owner = ProbeChild::new(child, capture_stdout)?;
    loop {
        if let Err(reason) = ensure_continues(should_continue, deadline) {
            owner.terminate()?;
            return Err(reason);
        }
        match owner.try_wait() {
            Ok(Some(status)) => return owner.finish(status).map(Some),
            Ok(None) => std::thread::sleep(POLL_INTERVAL),
            Err(error) => {
                owner.terminate()?;
                return Err(error);
            }
        }
    }
}

struct ProbeChild {
    child: Option<Child>,
    process_group: i32,
    stdout: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
}

impl ProbeChild {
    fn new(mut child: Child, capture_stdout: bool) -> Result<Self, MacosError> {
        let process_group = i32::try_from(child.id()).map_err(|_| {
            let _ = child.kill();
            let _ = child.wait();
            probe_failed("native probe subprocess identity is not representable")
        })?;
        let stdout = if capture_stdout {
            let Some(mut stdout) = child.stdout.take() else {
                let _ = child.kill();
                child.wait().map_err(|source| {
                    probe_io("native probe subprocess could not be reaped", source)
                })?;
                return Err(probe_failed(
                    "native probe subprocess did not retain its output pipe",
                ));
            };
            Some(std::thread::spawn(move || {
                let mut retained = Vec::with_capacity(1_025);
                stdout
                    .take(MAX_VERSION_OUTPUT_BYTES.saturating_add(1))
                    .read_to_end(&mut retained)?;
                Ok(retained)
            }))
        } else {
            None
        };
        Ok(Self { child: Some(child), process_group, stdout })
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>, MacosError> {
        self.child
            .as_mut()
            .ok_or_else(|| probe_failed("native probe subprocess ownership was lost"))?
            .try_wait()
            .map_err(|source| probe_io("native probe subprocess state is indeterminate", source))
    }

    fn finish(&mut self, status: ExitStatus) -> Result<ProbeOutput, MacosError> {
        self.child.take();
        let stdout = self.join_stdout()?;
        Ok(ProbeOutput { status, stdout })
    }

    fn terminate(&mut self) -> Result<(), MacosError> {
        if let Some(mut child) = self.child.take() {
            // SAFETY: every probe subprocess is created as leader of this fresh owned group.
            let _ = unsafe { libc::killpg(self.process_group, libc::SIGKILL) };
            child
                .wait()
                .map_err(|source| probe_io("native probe subprocess could not be reaped", source))?;
        }
        self.join_stdout().map(drop)
    }

    fn kill_process_group(&mut self) -> Result<(), MacosError> {
        if self.child.is_none() {
            return Err(probe_failed("process-group probe ownership was lost"));
        }
        // SAFETY: the child was created as leader of this fresh probe-owned process group.
        if unsafe { libc::killpg(self.process_group, libc::SIGKILL) } != 0 {
            let source = std::io::Error::last_os_error();
            self.terminate()?;
            return Err(probe_io("fresh process group could not be signalled", source));
        }
        let mut child = self
            .child
            .take()
            .ok_or_else(|| probe_failed("process-group probe ownership was lost"))?;
        child
            .wait()
            .map_err(|source| probe_io("process-group probe root could not be reaped", source))?;
        self.join_stdout().map(drop)
    }

    fn join_stdout(&mut self) -> Result<Vec<u8>, MacosError> {
        match self.stdout.take() {
            Some(reader) => reader
                .join()
                .map_err(|_| probe_failed("native probe output reader panicked"))?
                .map_err(|source| probe_io("native probe output could not be read", source)),
            None => Ok(Vec::new()),
        }
    }
}

impl Drop for ProbeChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // SAFETY: every probe subprocess is created as leader of this fresh owned group.
            let _ = unsafe { libc::killpg(self.process_group, libc::SIGKILL) };
            let _ = child.wait();
        }
        if let Some(reader) = self.stdout.take() {
            let _ = reader.join();
        }
    }
}

struct ProbeOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
}

fn probe_process_containment(
    timeout: Option<Duration>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<bool, MacosError> {
    use std::os::unix::process::CommandExt as _;

    ensure_continues(should_continue, None)?;
    let deadline = operation_deadline(timeout)?;
    let child = match Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(probe_io("process-group probe root could not be created", source)),
    };
    let mut owner = ProbeChild::new(child, false)?;
    if let Err(reason) = ensure_continues(should_continue, deadline) {
        owner.terminate()?;
        return Err(reason);
    }
    let pid = i32::try_from(
        owner
            .child
            .as_ref()
            .ok_or_else(|| probe_failed("process-group probe ownership was lost"))?
            .id(),
    )
    .map_err(|_| probe_failed("process-group probe identity is not representable"))?;
    // SAFETY: getpgid observes only the live child identity owned above.
    if unsafe { libc::getpgid(pid) } != pid {
        owner.terminate()?;
        return Ok(false);
    }
    owner.kill_process_group()?;
    Ok(true)
}

fn probe_proxy(
    endpoint: SocketAddr,
    timeout: Option<Duration>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<bool, MacosError> {
    ensure_continues(should_continue, None)?;
    let deadline = operation_deadline(timeout)?;
    let domain = if endpoint.is_ipv4() { libc::AF_INET } else { libc::AF_INET6 };
    // SAFETY: socket creates one new descriptor for the closed domain/type/protocol tuple.
    let descriptor = unsafe { libc::socket(domain, libc::SOCK_STREAM, 0) };
    if descriptor < 0 {
        return Err(probe_io(
            "managed proxy probe socket could not be created",
            std::io::Error::last_os_error(),
        ));
    }
    // SAFETY: socket returned one new uniquely owned descriptor.
    let socket = unsafe { OwnedFd::from_raw_fd(descriptor) };
    // SAFETY: fcntl acts only on the live probe-owned socket.
    let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(probe_io(
            "managed proxy probe socket could not be made cancellable",
            std::io::Error::last_os_error(),
        ));
    }
    if connect_nonblocking(socket.as_raw_fd(), endpoint) {
        return Ok(true);
    }
    loop {
        ensure_continues(should_continue, deadline)?;
        let mut poll = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: poll receives one initialized descriptor record and retains nothing.
        let ready = unsafe { libc::poll(&raw mut poll, 1, 0) };
        if ready < 0 {
            let source = std::io::Error::last_os_error();
            if source.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(probe_io("managed proxy probe socket cannot be observed", source));
        }
        if ready > 0 {
            let mut error = 0_i32;
            let mut length = u32::try_from(std::mem::size_of::<i32>()).unwrap_or(u32::MAX);
            // SAFETY: both output pointers name initialized writable storage of the supplied size.
            if unsafe {
                libc::getsockopt(
                    socket.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    (&raw mut error).cast(),
                    &raw mut length,
                )
            } != 0
            {
                return Err(probe_io(
                    "managed proxy probe result cannot be inspected",
                    std::io::Error::last_os_error(),
                ));
            }
            return Ok(error == 0);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn connect_nonblocking(descriptor: i32, endpoint: SocketAddr) -> bool {
    let status = match endpoint {
        SocketAddr::V4(endpoint) => {
            let address = libc::sockaddr_in {
                sin_len: u8::try_from(std::mem::size_of::<libc::sockaddr_in>()).unwrap_or(u8::MAX),
                sin_family: u8::try_from(libc::AF_INET).unwrap_or(u8::MAX),
                sin_port: endpoint.port().to_be(),
                sin_addr: libc::in_addr { s_addr: u32::from_ne_bytes(endpoint.ip().octets()) },
                sin_zero: [0; 8],
            };
            // SAFETY: address is initialized for AF_INET and size matches its type.
            unsafe {
                libc::connect(
                    descriptor,
                    (&raw const address).cast(),
                    u32::try_from(std::mem::size_of_val(&address)).unwrap_or(u32::MAX),
                )
            }
        }
        SocketAddr::V6(endpoint) => {
            let address = libc::sockaddr_in6 {
                sin6_len: u8::try_from(std::mem::size_of::<libc::sockaddr_in6>()).unwrap_or(u8::MAX),
                sin6_family: u8::try_from(libc::AF_INET6).unwrap_or(u8::MAX),
                sin6_port: endpoint.port().to_be(),
                sin6_flowinfo: endpoint.flowinfo(),
                sin6_addr: libc::in6_addr { s6_addr: endpoint.ip().octets() },
                sin6_scope_id: endpoint.scope_id(),
            };
            // SAFETY: address is initialized for AF_INET6 and size matches its type.
            unsafe {
                libc::connect(
                    descriptor,
                    (&raw const address).cast(),
                    u32::try_from(std::mem::size_of_val(&address)).unwrap_or(u32::MAX),
                )
            }
        }
    };
    status == 0
}

fn probe_resource_enforcement() -> bool {
    if !crate::resource_monitor::native_controls_available() {
        return false;
    }
    let mut major = 0;
    let mut minor = 0;
    // SAFETY: the pointers name initialized writable integers and libproc retains neither.
    if unsafe { libc::proc_libversion(&raw mut major, &raw mut minor) } != 0 || major < 1 {
        return false;
    }
    // SAFETY: a null buffer with size zero requests the kernel PID-list byte requirement.
    if unsafe { libc::proc_listpids(libc::PROC_ALL_PIDS, 0, core::ptr::null_mut(), 0) } <= 0 {
        return false;
    }
    [libc::RLIMIT_CPU, libc::RLIMIT_AS, libc::RLIMIT_NOFILE]
        .into_iter()
        .all(|resource| {
            let mut current = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            // SAFETY: current is writable and resource is from the closed constants above.
            if unsafe { libc::getrlimit(resource, &raw mut current) } != 0 {
                return false;
            }
            // SAFETY: reinstalling the observed limit exercises the activation setrlimit path
            // without widening or narrowing this probe process.
            unsafe { libc::setrlimit(resource, &raw const current) } == 0
        })
}

fn operation_deadline(timeout: Option<Duration>) -> Result<Option<Instant>, MacosError> {
    timeout
        .map(|timeout| {
            Instant::now()
                .checked_add(timeout)
                .ok_or_else(|| probe_failed("probe operation timeout is not representable"))
        })
        .transpose()
}

fn ensure_continues(
    should_continue: &mut impl FnMut() -> bool,
    deadline: Option<Instant>,
) -> Result<(), MacosError> {
    if !should_continue() {
        return Err(super::probe_cancelled());
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(MacosError::new(
            MacosErrorKind::ProbeFailed,
            MacosOperation::Probe,
            RecoveryAction::SelectSupportedBackend,
            "native capability probe exceeded its caller-selected operation timeout",
        ));
    }
    Ok(())
}

fn probe_io(detail: &'static str, source: std::io::Error) -> MacosError {
    MacosError::new(
        MacosErrorKind::ProbeFailed,
        MacosOperation::Probe,
        RecoveryAction::SelectSupportedBackend,
        detail,
    )
    .with_source(MacosErrorSource::Io(source.kind()))
}

fn probe_failed(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::ProbeFailed,
        MacosOperation::Probe,
        RecoveryAction::SelectSupportedBackend,
        detail,
    )
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
