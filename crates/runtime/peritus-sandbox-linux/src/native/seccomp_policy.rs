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
    "copy_file_range",
    "close",
    "close_range",
    "dup",
    "dup3",
    "fcntl",
    // Build tools use advisory locks to coordinate files already admitted by Landlock. This
    // syscall cannot discover or open another path and does not widen filesystem authority.
    "flock",
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
    "getpgrp",
    "getpgid",
    "getsid",
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
    "restart_syscall",
    "futex",
    "set_tid_address",
    "set_robust_list",
    "rseq",
    "sched_yield",
    "sched_getparam",
    "sched_getscheduler",
    "sched_getaffinity",
    "sched_setaffinity",
    "getpriority",
    "setpriority",
    "prctl",
    "setpgid",
    "setsid",
    "getrandom",
    "pipe2",
    "ppoll",
    "pselect6",
    "epoll_create1",
    "epoll_ctl",
    "epoll_wait",
    "epoll_pwait",
    "eventfd2",
    "timerfd_create",
    "timerfd_settime",
    "timerfd_gettime",
    "inotify_init1",
    "inotify_add_watch",
    "inotify_rm_watch",
    "memfd_create",
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
    "statfs",
    "fstatfs",
    "getresuid",
    "getresgid",
    "capset",
    "capget",
    "landlock_create_ruleset",
    "io_uring_setup",
    "io_uring_enter",
    "io_uring_register",
    "pidfd_open",
    "pidfd_send_signal",
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
    let filter = compile(architecture)?;
    seccompiler::apply_filter(&filter).map_err(|_| seccomp_error("seccomp-BPF installation failed"))
}

fn compile(architecture: TargetArch) -> Result<seccompiler::BpfProgram, LinuxError> {
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
        // AArch64 uses the *at forms from the common policy and does not expose several legacy
        // path syscalls. On x86-64, libc and language runtimes still legitimately select these
        // forms. Landlock independently confines every referenced path, so admitting the syscall
        // class does not widen the checked filesystem contract.
        json.push_str(
            ",{\"syscall\":\"dup2\"},{\"syscall\":\"vfork\"},{\"syscall\":\"poll\"},{\"syscall\":\"arch_prctl\"},\
             {\"syscall\":\"creat\"},{\"syscall\":\"link\"},{\"syscall\":\"symlink\"},{\"syscall\":\"inotify_init\"},\
             {\"syscall\":\"pkey_alloc\"},{\"syscall\":\"pkey_free\"},{\"syscall\":\"pkey_mprotect\"},\
             {\"syscall\":\"open\"},{\"syscall\":\"stat\"},{\"syscall\":\"lstat\"},\
             {\"syscall\":\"access\"},{\"syscall\":\"readlink\"},{\"syscall\":\"mkdir\"},\
             {\"syscall\":\"rmdir\"},{\"syscall\":\"unlink\"},{\"syscall\":\"rename\"},{\"syscall\":\"chmod\"},\
             {\"syscall\":\"chown\"},{\"syscall\":\"lchown\"}",
        );
    }
    json.push_str("]}}");
    let mut filters = seccompiler::compile_from_json(json.as_bytes(), architecture)
        .map_err(|_| seccomp_error("fixed seccomp policy compilation failed"))?;
    filters
        .remove("target")
        .ok_or_else(|| seccomp_error("fixed seccomp policy compiler returned no target"))
}

fn seccomp_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::SandboxDenied,
        LinuxOperation::Activate,
        LinuxRecovery::CancelAndReap,
        detail,
    )
}
