//! Windows process and Job Object containment for durable Git effects.

#![allow(
    unsafe_code,
    reason = "native birth-token and Windows Job Object calls are the narrow ownership boundary"
)]

use core::{ffi::c_void, mem::size_of, ptr};
use std::{
    ffi::OsStr,
    os::windows::{ffi::OsStrExt, io::AsRawHandle, process::CommandExt},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, FILETIME, GetLastError,
        HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            OpenJobObjectW, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
            GetCurrentProcess, GetProcessTimes, INFINITE, OpenProcess,
            PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
        },
    },
};

use super::super::OwnerBinding;
use crate::error::{Result, problem, uncertain};
const JOB_OBJECT_TERMINATE: u32 = 0x0008;
pub(in crate::git::effects) struct Containment(HANDLE);
pub(in crate::git::effects) struct Watchdog;
impl Drop for Containment {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the non-null job handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
pub(in crate::git::effects) fn configure_owner_command(command: &mut tokio::process::Command) {
    command
        .as_std_mut()
        .creation_flags(
            CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
        );
}
pub(in crate::git::effects) fn configure_git_child(command: &mut std::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}
pub(in crate::git::effects) fn configure_command_runner(command: &mut std::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}
pub(in crate::git::effects) fn command_binding(job_name: &str) -> Result<OwnerBinding> {
    binding(std::process::id(), Some(job_name.to_owned()))
}
pub(in crate::git::effects) fn activate_command(
    expected: &OwnerBinding,
    job_name: &str,
) -> Result<Containment> {
    let (containment, observed) = activate_owner(job_name)?;
    if &observed != expected {
        return Err(uncertain(
            "The Windows Git command helper changed identity before containment",
        ));
    }
    Ok(containment)
}
pub(in crate::git::effects) fn end_command(containment: Containment) -> ! {
    // SAFETY: this named nested Job contains the command helper and every Git descendant.
    let _ = unsafe { TerminateJobObject(containment.0, 1) };
    std::process::abort()
}
pub(in crate::git::effects) fn terminate_command(binding: &OwnerBinding) -> Result<()> {
    let name = binding.job_name.as_deref().ok_or_else(|| {
        uncertain("The Windows Git command has no durable Job Object identity")
    })?;
    let wide = wide(name);
    // SAFETY: requested access is limited to termination and `wide` is nul-terminated.
    let raw = unsafe { OpenJobObjectW(JOB_OBJECT_TERMINATE, 0, wide.as_ptr()) };
    if raw.is_null() {
        // SAFETY: sampled immediately after OpenJobObjectW failed.
        return if unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(uncertain("The exact Windows Git command Job could not be opened"))
        };
    }
    let job = Containment(raw);
    // SAFETY: `job` owns the exact named Job retained in the command helper receipt.
    if unsafe { TerminateJobObject(job.0, 1) } == 0 {
        return Err(uncertain("The exact Windows Git command Job could not be terminated"));
    }
    Ok(())
}
pub(in crate::git::effects) fn validate_owned_command(
    binding: &OwnerBinding,
    child: &std::process::Child,
) -> Result<()> {
    validate_owned_child(binding, child)?;
    let handle = child.as_raw_handle().cast();
    if handle_start_token(handle) != Some(binding.start_token) {
        return Err(uncertain(
            "The owned Windows Git command child does not match its retained birth identity",
        ));
    }
    Ok(())
}
pub(in crate::git::effects) fn owned_command_exited(
    binding: &OwnerBinding,
    child: &std::process::Child,
) -> Result<bool> {
    validate_owned_child(binding, child)?;
    // SAFETY: Child owns this exact process handle for the duration of the zero-time wait.
    match unsafe { WaitForSingleObject(child.as_raw_handle().cast(), 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(uncertain(
            "The owned Windows Git command child exit could not be observed",
        )),
    }
}
pub(in crate::git::effects) fn terminate_owned_command(
    binding: &OwnerBinding,
    child: &std::process::Child,
) -> Result<()> {
    validate_owned_child(binding, child)?;
    terminate_command(binding)
}
pub(in crate::git::effects) fn drain_command(
    binding: &OwnerBinding,
    child: &mut std::process::Child,
) -> Result<()> {
    const SYNCHRONIZE: u32 = 0x0010_0000;
    let name = binding.job_name.as_deref().ok_or_else(|| {
        uncertain("The Windows Git command has no durable Job Object identity")
    })?;
    let wide = wide(name);
    // SAFETY: requested access is synchronization-only and `wide` is nul-terminated.
    let raw = unsafe { OpenJobObjectW(SYNCHRONIZE, 0, wide.as_ptr()) };
    if raw.is_null() {
        // SAFETY: sampled immediately after OpenJobObjectW failed.
        return if unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND {
            child.wait().map(|_| ()).map_err(problem)
        } else {
            Err(uncertain("The exact Windows Git command Job could not be reopened"))
        };
    }
    let job = Containment(raw);
    // SAFETY: the owned Job handle stays live for the unbounded exact-drain wait.
    if unsafe { WaitForSingleObject(job.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(uncertain("The exact Windows Git command Job could not be drained"));
    }
    child.wait().map(|_| ()).map_err(problem)
}
pub(in crate::git::effects) fn complete(_containment: &mut Containment) {
    // Closing the completed owner's kill-on-close job also reaps any unexpected survivor.
}
pub(in crate::git::effects) fn start_watchdog(
    _directory: &std::path::Path,
    _binding: &OwnerBinding,
    _flag: &str,
) -> Result<Watchdog> {
    Ok(Watchdog)
}
pub(in crate::git::effects) fn finish_watchdog(_watchdog: Watchdog) -> Result<()> {
    Ok(())
}

pub(in crate::git::effects) fn validate_watchdog(_binding: &OwnerBinding) -> Result<()> {
    Err(problem("The Windows Git owner uses a Job Object instead of a watchdog process"))
}

pub(in crate::git::effects) fn watchdog_terminate(_binding: &OwnerBinding) -> ! {
    std::process::abort()
}

pub(in crate::git::effects) fn launcher_binding(pid: u32) -> Result<OwnerBinding> {
    binding(pid, None)
}

pub(in crate::git::effects) fn activate_owner(job_name: &str) -> Result<(Containment, OwnerBinding)> {
    let wide = wide(job_name);
    // SAFETY: null security attributes request defaults and `wide` is nul-terminated.
    let raw = unsafe { CreateJobObjectW(ptr::null(), wide.as_ptr()) };
    if raw.is_null() {
        return Err(problem("The Windows Git Job Object could not be created"));
    }
    // SAFETY: sampled immediately after CreateJobObjectW.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: CreateJobObjectW returned this owned handle.
        let _ = unsafe { CloseHandle(raw) };
        return Err(uncertain("The Windows Git Job Object identity is already active"));
    }
    let containment = Containment(raw);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let length = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
        .map_err(problem)?;
    // SAFETY: the job and correctly-sized immutable limit record remain live for this call.
    if unsafe {
        SetInformationJobObject(
            containment.0,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast::<c_void>(),
            length,
        )
    } == 0
    {
        return Err(problem("The Windows Git Job Object kill-on-close policy failed"));
    }
    // SAFETY: the pseudo-handle names the current process and the owned job remains live.
    if unsafe { AssignProcessToJobObject(containment.0, GetCurrentProcess()) } == 0 {
        return Err(problem("The Windows Git owner could not enter its Job Object"));
    }
    let binding = binding(std::process::id(), Some(job_name.to_owned()))?;
    Ok((containment, binding))
}

pub(in crate::git::effects) fn terminate(binding: &OwnerBinding) -> Result<()> {
    let name = binding
        .job_name
        .as_deref()
        .ok_or_else(|| uncertain("The Windows Git owner has no durable Job Object identity"))?;
    let wide = wide(name);
    // SAFETY: requested access is limited to termination and `wide` is nul-terminated.
    let raw = unsafe { OpenJobObjectW(JOB_OBJECT_TERMINATE, 0, wide.as_ptr()) };
    if raw.is_null() {
        // SAFETY: sampled immediately after OpenJobObjectW failed. Completion may win the
        // observe-then-terminate race; the caller will reconcile the terminal receipt.
        return if unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(uncertain("The exact Windows Git Job Object cannot be reopened"))
        };
    }
    let job = Containment(raw);
    // SAFETY: `job` owns the exact named Job Object retained in the durable owner receipt.
    if unsafe { TerminateJobObject(job.0, 1) } == 0 {
        return Err(uncertain("The exact Windows Git Job Object could not be terminated"));
    }
    Ok(())
}

pub(in crate::git::effects) fn validate_launcher_binding(binding: &OwnerBinding) -> Result<()> {
    if !binding.complete_containment
        || binding.process_group.is_some()
        || binding.job_name.is_some()
    {
        return Err(uncertain(
            "The retained pre-activation Windows Git owner has an invalid containment receipt",
        ));
    }
    Ok(())
}

pub(in crate::git::effects) fn validate_owner_binding(binding: &OwnerBinding, job_name: &str) -> Result<()> {
    if !binding.complete_containment
        || binding.process_group.is_some()
        || binding.job_name.as_deref() != Some(job_name)
    {
        return Err(uncertain(
            "The retained Windows Git owner has an invalid Job Object receipt",
        ));
    }
    Ok(())
}

fn validate_owned_child(binding: &OwnerBinding, child: &std::process::Child) -> Result<()> {
    if child.id() != binding.pid {
        return Err(uncertain(
            "The retained Windows Git command binding does not belong to the owned child",
        ));
    }
    Ok(())
}

fn binding(pid: u32, job_name: Option<String>) -> Result<OwnerBinding> {
    let start_token = start_token(pid)
        .ok_or_else(|| uncertain("The Windows Git owner birth identity could not be observed"))?;
    Ok(OwnerBinding {
        pid,
        start_token,
        process_group: None,
        complete_containment: true,
        job_name,
    })
}

fn start_token(pid: u32) -> Option<u64> {
    // SAFETY: inheritance is disabled and the PID is passed by value.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let token = handle_start_token(handle);
    // SAFETY: OpenProcess returned this uniquely owned handle.
    let _ = unsafe { CloseHandle(handle) };
    token
}

fn handle_start_token(handle: HANDLE) -> Option<u64> {
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: each pointer names initialized writable storage and the query handle is live.
    let observed = unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    (observed != 0)
        .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
