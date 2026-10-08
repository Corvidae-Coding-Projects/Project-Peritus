//! Unix process-group containment for durable Git effects.

#![allow(
    unsafe_code,
    reason = "native birth-token and process-group calls are the narrow ownership boundary"
)]

use super::super::OwnerBinding;
use crate::error::{Result, problem, uncertain};
#[cfg(unix)]
use peritus_process::{NativeProcessProbe, ProbeObservation, ProcessProbe};
#[cfg(unix)]
use std::{
    mem::MaybeUninit,
    path::Path,
    process::{Child, Stdio},
};

#[cfg(unix)]
pub(in crate::git::effects) struct Containment {
    group: i32,
    armed: bool,
}

#[cfg(unix)]
pub(in crate::git::effects) struct Watchdog(Child);

#[cfg(unix)]
impl Drop for Containment {
    fn drop(&mut self) {
        if self.armed {
            // SAFETY: this guard owns a dedicated group whose numeric identity is the retained
            // owner PID. SIGKILL is fail-closed after an owner-side publication failure.
            let _ = unsafe { libc::kill(-self.group, libc::SIGKILL) };
        }
    }
}

#[cfg(unix)]
pub(in crate::git::effects) fn configure_owner_command(command: &mut tokio::process::Command) {
    use std::os::unix::process::CommandExt;
    command.as_std_mut().process_group(0);
}

#[cfg(unix)]
pub(in crate::git::effects) fn configure_git_child(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: getpgrp has no pointer arguments and only observes the calling process.
    let group = unsafe { libc::getpgrp() };
    command.process_group(group);
}

#[cfg(unix)]
pub(in crate::git::effects) fn configure_command_runner(command: &mut std::process::Command) {
    command.stdin(Stdio::null());
}

#[cfg(unix)]
pub(in crate::git::effects) fn command_binding(_job_name: &str) -> Result<OwnerBinding> {
    binding(std::process::id(), None)
}

#[cfg(unix)]
pub(in crate::git::effects) fn activate_command(
    expected: &OwnerBinding,
    _job_name: &str,
) -> Result<Containment> {
    let current = binding(std::process::id(), None)?;
    if &current != expected {
        return Err(uncertain("The Git command helper changed identity before containment"));
    }
    // SAFETY: both zero arguments select the current process and a new group led by its PID.
    if unsafe { libc::setpgid(0, 0) } != 0 {
        return Err(problem("The Git command helper could not enter its dedicated process group"));
    }
    // SAFETY: getpgrp has no pointer arguments and only observes the calling process.
    let group = unsafe { libc::getpgrp() };
    if group <= 0 || u32::try_from(group).ok() != Some(expected.pid) {
        return Err(problem("The Git command helper entered the wrong process group"));
    }
    Ok(Containment { group, armed: true })
}

#[cfg(unix)]
pub(in crate::git::effects) fn end_command(mut containment: Containment) -> ! {
    containment.armed = false;
    // SAFETY: the current command helper pins this exact group identity until SIGKILL lands.
    let _ = unsafe { libc::kill(-containment.group, libc::SIGKILL) };
    std::process::abort()
}

#[cfg(unix)]
pub(in crate::git::effects) fn terminate_command(binding: &OwnerBinding) -> Result<()> {
    validate_unix_binding(binding)?;
    let mut probe = NativeProcessProbe::new();
    match probe.observe(binding.identity()).map_err(uncertain)? {
        ProbeObservation::ExactAbsent => return Ok(()),
        ProbeObservation::ExactLive => {}
        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
            return Err(uncertain(
                "The exact Git command identity cannot be verified for group termination",
            ));
        }
    }
    let group = i32::try_from(binding.pid)
        .map_err(|_| uncertain("The Git command process group is not representable"))?;
    // SAFETY: the immediately preceding exact-birth observation proves that the unreaped helper
    // still pins this root-led group identity for this one termination request.
    if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(uncertain(format!(
                "The exact Git command process group could not be terminated: {error}",
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(in crate::git::effects) fn validate_owned_command(binding: &OwnerBinding, child: &Child) -> Result<()> {
    validate_owned_child(binding, child)?;
    if start_token(binding.pid) != Some(binding.start_token) {
        return Err(uncertain(
            "The owned Git command child does not match its retained birth identity",
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(in crate::git::effects) fn owned_command_exited(binding: &OwnerBinding, child: &Child) -> Result<bool> {
    validate_owned_child(binding, child)?;
    let mut information = MaybeUninit::<libc::siginfo_t>::zeroed();
    let identifier = libc::id_t::try_from(binding.pid)
        .map_err(|_| uncertain("The owned Git command PID is not representable"))?;
    // SAFETY: `information` points to writable siginfo storage. WNOWAIT observes only this exact
    // owned child and deliberately leaves it unreaped so its PID/group identity cannot be reused.
    if unsafe {
        libc::waitid(
            libc::P_PID,
            identifier,
            information.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    } != 0
    {
        return Err(uncertain(format!(
            "The owned Git command child exit could not be observed: {}",
            std::io::Error::last_os_error(),
        )));
    }
    // SAFETY: waitid succeeded and therefore initialized the siginfo record. A zero signal means
    // WNOHANG found no waitable exit event; WEXITED excludes stop and continue notifications.
    Ok(unsafe { information.assume_init() }.si_signo != 0)
}

#[cfg(unix)]
pub(in crate::git::effects) fn terminate_owned_command(binding: &OwnerBinding, child: &Child) -> Result<()> {
    validate_owned_child(binding, child)?;
    let group = i32::try_from(binding.pid)
        .map_err(|_| uncertain("The Git command process group is not representable"))?;
    // SAFETY: the retained, unreaped owned child pins the exact helper PID/group identity. This
    // remains true after exit and avoids relying on probes that may classify a zombie as absent.
    if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(uncertain(format!(
                "The owned Git command process group could not be terminated: {error}",
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(in crate::git::effects) fn drain_command(binding: &OwnerBinding, child: &mut Child) -> Result<()> {
    validate_unix_binding(binding)?;
    if child.id() != binding.pid {
        return Err(uncertain("The owned Git command child changed before it was reaped"));
    }
    child
        .wait()
        .map(|_| ())
        .map_err(|error| uncertain(format!("The owned Git command child could not be reaped: {error}")))
}

#[cfg(unix)]
pub(in crate::git::effects) fn launcher_binding(pid: u32) -> Result<OwnerBinding> {
    binding(pid, None)
}

#[cfg(unix)]
pub(in crate::git::effects) fn activate_owner(_job_name: &str) -> Result<(Containment, OwnerBinding)> {
    let pid = std::process::id();
    // SAFETY: getpgrp has no pointer arguments and only observes the calling process.
    let group = unsafe { libc::getpgrp() };
    if group <= 0 || u32::try_from(group).ok() != Some(pid) {
        return Err(problem("The Git owner did not enter its dedicated process group"));
    }
    Ok((Containment { group, armed: true }, binding(pid, None)?))
}

#[cfg(unix)]
pub(in crate::git::effects) fn complete(containment: &mut Containment) {
    containment.armed = false;
}

#[cfg(unix)]
pub(in crate::git::effects) fn start_watchdog(
    directory: &Path,
    binding: &OwnerBinding,
    flag: &str,
) -> Result<Watchdog> {
    use std::os::unix::process::CommandExt;

    let group = i32::try_from(binding.pid)
        .map_err(|_| problem("The Git watchdog process group is not representable"))?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg(flag)
        .arg(directory)
        .process_group(group)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(Watchdog(command.spawn()?))
}

#[cfg(unix)]
pub(in crate::git::effects) fn finish_watchdog(mut watchdog: Watchdog) -> Result<()> {
    watchdog.0.wait().map(|_| ()).map_err(problem)
}

#[cfg(unix)]
pub(in crate::git::effects) fn validate_watchdog(binding: &OwnerBinding) -> Result<()> {
    // SAFETY: getpgrp has no pointer arguments and only observes the calling process.
    let group = unsafe { libc::getpgrp() };
    if group <= 0 || u32::try_from(group).ok() != Some(binding.pid) {
        return Err(problem("The Git watchdog does not belong to the retained owner group"));
    }
    Ok(())
}

#[cfg(unix)]
pub(in crate::git::effects) fn watchdog_terminate(binding: &OwnerBinding) -> ! {
    let group = i32::try_from(binding.pid).unwrap_or(i32::MAX);
    // SAFETY: the watchdog is itself a live member of this dedicated group, which prevents its
    // identity from being reused before this exact group-wide termination request.
    let _ = unsafe { libc::kill(-group, libc::SIGKILL) };
    std::process::abort()
}

#[cfg(unix)]
pub(in crate::git::effects) fn terminate(binding: &OwnerBinding) -> Result<()> {
    let mut probe = NativeProcessProbe::new();
    probe.terminate(binding.identity()).map_err(uncertain)
}

#[cfg(unix)]
pub(in crate::git::effects) fn validate_launcher_binding(binding: &OwnerBinding) -> Result<()> {
    validate_unix_binding(binding)
}

#[cfg(unix)]
pub(in crate::git::effects) fn validate_owner_binding(binding: &OwnerBinding, _job_name: &str) -> Result<()> {
    validate_unix_binding(binding)
}

#[cfg(unix)]
fn validate_unix_binding(binding: &OwnerBinding) -> Result<()> {
    if !binding.complete_containment
        || binding.process_group != Some(binding.pid)
        || binding.job_name.is_some()
    {
        return Err(uncertain("The retained Git owner has an invalid Unix containment receipt"));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_owned_child(binding: &OwnerBinding, child: &Child) -> Result<()> {
    validate_unix_binding(binding)?;
    if child.id() != binding.pid {
        return Err(uncertain(
            "The retained Git command binding does not belong to the owned child",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn binding(pid: u32, job_name: Option<String>) -> Result<OwnerBinding> {
    let start_token = start_token(pid)
        .ok_or_else(|| uncertain("The Git owner birth identity could not be observed"))?;
    Ok(OwnerBinding {
        pid,
        start_token,
        process_group: Some(pid),
        complete_containment: true,
        job_name,
    })
}

#[cfg(target_os = "linux")]
fn start_token(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    text.get(close + 2..)?
        .split_ascii_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

#[cfg(target_os = "macos")]
fn start_token(pid: u32) -> Option<u64> {
    use core::{ffi::c_void, mem::{MaybeUninit, size_of}};

    let pid = i32::try_from(pid).ok()?;
    let size = i32::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    let mut information = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    // SAFETY: the buffer has the exact requested proc_bsdinfo size and is read only after the
    // kernel reports that complete size.
    let observed = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            information.as_mut_ptr().cast::<c_void>(),
            size,
        )
    };
    if observed != size {
        return None;
    }
    // SAFETY: proc_pidinfo reported complete initialization above.
    let information = unsafe { information.assume_init() };
    if information.pbi_start_tvusec >= 1_000_000 {
        return None;
    }
    information
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|seconds| seconds.checked_add(information.pbi_start_tvusec))
}
