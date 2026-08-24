//! Reviewed fixed syscall-class seccomp compiler and installer.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
use seccompiler::TargetArch;

const BASE_SYSCALLS: &[&str] = &[
    "read",
    "write",
    "readv",
    "writev",
    "pread64",
    "pwrite64",
    "close",
    "close_range",
    "dup",
    "dup2",
    "dup3",
    "fcntl",
    "ioctl",
    "lseek",
    "fstat",
    "newfstatat",
    "statx",
    "getdents64",
    "readlinkat",
    "faccessat",
    "faccessat2",
    "openat",
    "openat2",
    "mkdirat",
    "mknodat",
    "unlinkat",
    "renameat2",
    "linkat",
    "symlinkat",
    "fchmod",
    "fchmodat",
    "fchownat",
    "truncate",
    "ftruncate",
    "fsync",
    "fdatasync",
    "utimensat",
    "mmap",
    "mprotect",
    "munmap",
    "mremap",
    "madvise",
    "brk",
    "rt_sigaction",
    "rt_sigprocmask",
    "rt_sigreturn",
    "sigaltstack",
    "getpid",
    "getppid",
    "gettid",
    "getuid",
    "geteuid",
    "getgid",
    "getegid",
    "getgroups",
    "uname",
    "sysinfo",
    "getcwd",
    "chdir",
    "umask",
    "getrlimit",
    "prlimit64",
    "getrusage",
    "clock_gettime",
    "clock_nanosleep",
    "nanosleep",
    "futex",
    "set_tid_address",
    "set_robust_list",
    "rseq",
    "sched_yield",
    "sched_getaffinity",
    "getrandom",
    "pipe2",
    "poll",
    "ppoll",
    "pselect6",
    "epoll_create1",
    "epoll_ctl",
    "epoll_pwait",
    "eventfd2",
    "timerfd_create",
    "timerfd_settime",
    "timerfd_gettime",
    "socket",
    "socketpair",
    "bind",
    "listen",
    "accept4",
    "connect",
    "shutdown",
    "getsockname",
    "getpeername",
    "setsockopt",
    "getsockopt",
    "sendto",
    "recvfrom",
    "sendmsg",
    "recvmsg",
    "clone",
    "clone3",
    "execve",
    "execveat",
    "wait4",
    "waitid",
    "kill",
    "tgkill",
    "exit",
    "exit_group",
];

pub(super) fn install() -> Result<(), LinuxError> {
    let architecture: TargetArch = std::env::consts::ARCH
        .try_into()
        .map_err(|_| seccomp_error("seccomp compiler does not support the current architecture"))?;
    let mut json = String::from(
        r#"{"target":{"mismatch_action":{"errno":1},"match_action":"allow","filter":["#,
    );
    for (index, syscall) in BASE_SYSCALLS.iter().enumerate() {
        if index != 0 {
            json.push(',');
        }
        json.push_str("{\"syscall\":\"");
        json.push_str(syscall);
        json.push_str("\"}");
    }
    if architecture == TargetArch::x86_64 {
        json.push_str(",{\"syscall\":\"arch_prctl\"}");
    }
    json.push_str("]}}");
    let filters = seccompiler::compile_from_json(json.as_bytes(), architecture)
        .map_err(|_| seccomp_error("fixed seccomp policy compilation failed"))?;
    let filter = filters
        .get("target")
        .ok_or_else(|| seccomp_error("fixed seccomp policy compiler returned no target"))?;
    seccompiler::apply_filter(filter).map_err(|_| seccomp_error("seccomp-BPF installation failed"))
}

fn seccomp_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::SandboxDenied,
        LinuxOperation::Activate,
        LinuxRecovery::CancelAndReap,
        detail,
    )
}
