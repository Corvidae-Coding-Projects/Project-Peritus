//! Linux session ownership across ordinary job-control process groups.

use std::{
    io::ErrorKind,
    os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd},
};

use nix::sys::signal::Signal;

use super::ProcessTreeIdentity;
use crate::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Member {
    pid: u32,
    session: u32,
    start: u64,
}

impl Member {
    pub(crate) const fn pid(self) -> u32 {
        self.pid
    }
}

/// Every Unix launch creates a root-led session. Bash may create additional process groups
/// inside it; those groups remain owned even after the root exits and their parent changes.
pub(crate) fn members(identity: ProcessTreeIdentity) -> Result<Vec<Member>, ProcessError> {
    let leader = identity
        .process_group()
        .filter(|leader| *leader != 0 && *leader == identity.root_pid())
        .ok_or_else(|| tree_error("owned Linux session identity is unavailable"))?;
    let start = identity
        .start_token()
        .ok_or_else(|| tree_error("owned Linux session birth identity is unavailable"))?;
    validate_root(identity, leader, start)?;
    let entries = std::fs::read_dir("/proc")
        .map_err(|_| tree_error("owned Linux session cannot be enumerated"))?;
    let mut members = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| tree_error("Linux process-table entry cannot be read"))?;
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else {
            continue;
        };
        let Some(member) = read_member(pid)? else { continue };
        if member.session == leader {
            if member.start < start || (member.pid == leader && member.start != start) {
                return Err(tree_error("owned Linux session birth identity changed"));
            }
            members.push(member);
        }
    }
    validate_root(identity, leader, start)?;
    Ok(members)
}

fn validate_root(
    identity: ProcessTreeIdentity,
    leader: u32,
    start: u64,
) -> Result<(), ProcessError> {
    if let Some(root) = read_member(identity.root_pid())?
        && (root.start != start || root.session != leader)
    {
        return Err(tree_error("owned Linux session root identity changed"));
    }
    Ok(())
}

fn read_member(pid: u32) -> Result<Option<Member>, ProcessError> {
    let bytes = match std::fs::read(format!("/proc/{pid}/stat")) {
        Ok(bytes) => bytes,
        Err(error)
            if error.kind() == ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            return Ok(None);
        }
        Err(_) => return Err(tree_error("Linux process birth identity cannot be read")),
    };
    // comm may contain non-UTF-8 bytes and closing parentheses. Only the numeric prefix and
    // the ASCII fields after its final delimiter are relevant to ownership.
    let open = bytes
        .iter()
        .position(|byte| *byte == b'(')
        .ok_or_else(|| tree_error("Linux process status is malformed"))?;
    let close = bytes
        .iter()
        .rposition(|byte| *byte == b')')
        .ok_or_else(|| tree_error("Linux process status is malformed"))?;
    let prefix = std::str::from_utf8(&bytes[..open])
        .map_err(|_| tree_error("Linux process PID is malformed"))?;
    if close <= open || prefix.trim().parse::<u32>().ok() != Some(pid) {
        return Err(tree_error("Linux process status does not match its PID"));
    }
    let fields = std::str::from_utf8(&bytes[close + 1..])
        .map_err(|_| tree_error("Linux process status fields are malformed"))?;
    let mut fields = fields.split_ascii_whitespace();
    let session = fields
        .nth(3)
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| tree_error("Linux process session identity is malformed"))?;
    let start = fields
        .nth(15)
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| tree_error("Linux process birth identity is malformed"))?;
    Ok(Some(Member { pid, session, start }))
}

pub(crate) fn signal(identity: ProcessTreeIdentity, signal: Signal) -> Result<(), ProcessError> {
    let mut owned = members(identity)?;
    // Signal children before the root. Each signal still uses its own guarded handle, and the
    // bounded reap loop rescans for any child created while this census was being taken.
    owned.sort_unstable_by_key(|member| member.pid == identity.root_pid());
    for member in owned {
        signal_member(member, signal)?;
    }
    Ok(())
}

#[allow(unsafe_code, reason = "pidfd syscalls bind signals to the observed Linux process birth")]
fn signal_member(member: Member, signal: Signal) -> Result<(), ProcessError> {
    let pid = i32::try_from(member.pid)
        .map_err(|_| tree_error("Linux process PID is not representable"))?;
    // SAFETY: pidfd_open takes a positive PID and zero flags, with no borrowed pointers. The
    // returned descriptor is uniquely owned below, then closed on every path by OwnedFd.
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0_u32) };
    if descriptor < 0 {
        return if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(tree_error("owned Linux process handle cannot be opened"))
        };
    }
    let descriptor = i32::try_from(descriptor)
        .map_err(|_| tree_error("Linux process handle is not representable"))?;
    // SAFETY: the successful pidfd_open syscall returned a new descriptor owned by this call;
    // no other owner closes it, and OwnedFd is constructed exactly once.
    let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };
    let Some(current) = read_member(member.pid)? else { return Ok(()) };
    if current.start != member.start || current.session != member.session {
        return Err(tree_error("owned Linux process changed before signalling"));
    }
    // SAFETY: the descriptor remains live and names the process pinned before its birth/session
    // recheck. Signal is a valid POSIX signal; null siginfo and zero flags request the ordinary
    // kernel-generated signal. PID reuse cannot redirect this handle to another process.
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            descriptor.as_raw_fd(),
            signal as i32,
            std::ptr::null::<libc::siginfo_t>(),
            0_u32,
        )
    };
    if result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(tree_error("owned Linux process signal failed"))
    }
}

const fn tree_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ProcessTree,
        ProcessOperation::Control,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
