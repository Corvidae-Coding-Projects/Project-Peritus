//! Reopenable named Job Object ownership for independently retained Windows effects.

#![allow(
    unsafe_code,
    reason = "Windows Job Object and process-handle calls are the narrow durable owner boundary"
)]

use core::{ffi::c_void, mem::size_of, ptr};
use std::{
    ffi::OsStr,
    fs::File,
    os::windows::{
        ffi::OsStrExt as _,
        io::{AsRawHandle as _, FromRawHandle as _},
        process::CommandExt as _,
    },
    process::Command,
};

use peritus_types::Sha256Digest;
use windows_sys::Win32::{
    Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_INVALID_PARAMETER, FILETIME,
        GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
            JobObjectExtendedLimitInformation, OpenJobObjectW, QueryInformationJobObject,
            SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, GetCurrentProcess,
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
        },
    },
};

use crate::{
    ErrorCode, NativeProcessProbe, NativeWindowsContainmentBinding,
    NativeWindowsContainmentIdentity, ProbeObservation, ProcessError, ProcessOperation,
    ProcessProbe, ProcessTreeIdentity, ProcessTreeQuiescence, RecoveryClass,
};

const JOB_OBJECT_QUERY: u32 = 0x0004;
const JOB_OBJECT_TERMINATE: u32 = 0x0008;
const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;

/// Live handle for one named kill-on-close Job Object owned by a detached effect process.
///
/// The detached owner enters the Job before publishing [`Self::identity`]. Descendants inherit
/// its Job membership, while a later launcher generation can reopen the exact name and combine
/// it with the persisted root birth token before observing or terminating the tree.
pub struct NativeWindowsProcessOwner {
    job: File,
    identity: NativeWindowsContainmentIdentity,
}

impl NativeWindowsProcessOwner {
    /// Configures a child that will become an independently retained Job Object owner.
    pub fn configure_detached_command(command: &mut Command) {
        command.creation_flags(CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP);
    }

    /// Creates the named kill-on-close Job and places the current process inside it.
    ///
    /// # Errors
    /// Rejects a reused name, malformed identity, failed Job policy, or unobservable membership.
    pub fn activate_current(
        job_identity: Sha256Digest,
        object_name: impl Into<String>,
    ) -> Result<Self, ProcessError> {
        let object_name = object_name.into();
        let start_token = crate::platform::current_start_token(std::process::id())
            .ok_or_else(|| spawn_error("Windows detached owner birth identity is unavailable"))?;
        let target = ProcessTreeIdentity::new(
            std::process::id(),
            Some(start_token),
            None,
            true,
        );
        let identity = NativeWindowsContainmentIdentity::new(
            job_identity,
            object_name.clone(),
            target,
        )?;
        let wide = wide(&object_name);
        // SAFETY: null security attributes and the terminated immutable name are valid inputs.
        let raw = unsafe { CreateJobObjectW(ptr::null(), wide.as_ptr()) };
        if raw.is_null() {
            return Err(spawn_cause(
                "Windows detached owner Job Object could not be created",
                std::io::Error::last_os_error(),
            ));
        }
        // SAFETY: sampled immediately after successful named-object creation on this thread.
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        // SAFETY: the new non-null handle transfers into exactly one File owner.
        let job = unsafe { File::from_raw_handle(raw.cast()) };
        if already_exists {
            return Err(owner_error(
                "Windows detached owner Job Object identity is already active",
            ));
        }
        install_kill_on_close(raw)?;
        // SAFETY: both the current-process pseudo-handle and the owned Job handle are live.
        if unsafe { AssignProcessToJobObject(raw, GetCurrentProcess()) } == 0 {
            return Err(spawn_cause(
                "Windows detached owner could not enter its Job Object",
                std::io::Error::last_os_error(),
            ));
        }
        if inspect_target(raw, target)? != ProbeObservation::ExactLive {
            return Err(owner_error(
                "Windows detached owner Job membership is not exactly observable",
            ));
        }
        Ok(Self { job, identity })
    }

    /// Returns the exact named Job and root birth identity held by this owner.
    #[must_use]
    pub const fn identity(&self) -> &NativeWindowsContainmentIdentity {
        &self.identity
    }

    /// Reopens and observes one persisted Job/root binding without granting PID-only authority.
    ///
    /// # Errors
    /// Returns an indeterminate recovery error when native observation itself fails.
    pub fn observe_durable(
        identity: &NativeWindowsContainmentIdentity,
    ) -> Result<ProbeObservation, ProcessError> {
        let Some(job) = open_job(
            identity.object_name(),
            JOB_OBJECT_QUERY | SYNCHRONIZE_ACCESS,
        )? else {
            let mut probe = NativeProcessProbe::new();
            return Ok(match probe.observe(identity.target_identity())? {
                ProbeObservation::ExactAbsent => ProbeObservation::ExactAbsent,
                ProbeObservation::Mismatched => ProbeObservation::Mismatched,
                ProbeObservation::ExactLive | ProbeObservation::Unverifiable => {
                    ProbeObservation::Unverifiable
                }
            });
        };
        let observed = inspect_target(job.as_raw_handle().cast(), identity.target_identity())?;
        if observed == ProbeObservation::ExactAbsent
            && active_processes(job.as_raw_handle().cast())? != 0
        {
            return Ok(ProbeObservation::Unverifiable);
        }
        Ok(observed)
    }

    /// Observes a pre-effect Job binding for which no target birth identity was committed.
    ///
    /// A missing or empty Job proves that this intended containment is quiescent. A live member
    /// remains unverifiable because the object name alone cannot reject Job-name reuse.
    ///
    /// # Errors
    /// Returns an indeterminate recovery error when Job observation itself fails.
    pub fn observe_durable_binding(
        binding: &NativeWindowsContainmentBinding,
    ) -> Result<ProbeObservation, ProcessError> {
        let Some(job) = open_job(
            binding.object_name(),
            JOB_OBJECT_QUERY | SYNCHRONIZE_ACCESS,
        )? else {
            return Ok(ProbeObservation::ExactAbsent);
        };
        Ok(if active_processes(job.as_raw_handle().cast())? == 0 {
            ProbeObservation::ExactAbsent
        } else {
            ProbeObservation::Unverifiable
        })
    }

    /// Establishes complete Job quiescence for one persisted exact owner.
    ///
    /// # Errors
    /// Returns an indeterminate recovery error when Job or root observation fails.
    pub fn observe_durable_quiescence(
        identity: &NativeWindowsContainmentIdentity,
    ) -> Result<ProcessTreeQuiescence, ProcessError> {
        let Some(job) = open_job(
            identity.object_name(),
            JOB_OBJECT_QUERY | SYNCHRONIZE_ACCESS,
        )? else {
            let mut probe = NativeProcessProbe::new();
            return Ok(if probe.observe(identity.target_identity())?
                == ProbeObservation::ExactAbsent
            {
                ProcessTreeQuiescence::Quiescent
            } else {
                ProcessTreeQuiescence::Unverifiable
            });
        };
        let target = inspect_target(job.as_raw_handle().cast(), identity.target_identity())?;
        Ok(if target == ProbeObservation::ExactAbsent
            && active_processes(job.as_raw_handle().cast())? == 0
        {
            ProcessTreeQuiescence::Quiescent
        } else {
            ProcessTreeQuiescence::Unverifiable
        })
    }

    /// Establishes quiescence for a pre-effect binding without attributing live name reuse.
    ///
    /// # Errors
    /// Returns an indeterminate recovery error when Job observation itself fails.
    pub fn observe_durable_binding_quiescence(
        binding: &NativeWindowsContainmentBinding,
    ) -> Result<ProcessTreeQuiescence, ProcessError> {
        Ok(if Self::observe_durable_binding(binding)? == ProbeObservation::ExactAbsent {
            ProcessTreeQuiescence::Quiescent
        } else {
            ProcessTreeQuiescence::Unverifiable
        })
    }

    /// Terminates only a reopened Job whose root still has the persisted exact birth identity.
    ///
    /// # Errors
    /// Refuses mismatched, absent-with-live-descendants, or unverifiable ownership.
    pub fn terminate_durable(
        identity: &NativeWindowsContainmentIdentity,
    ) -> Result<(), ProcessError> {
        let Some(job) = open_job(
            identity.object_name(),
            JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE | SYNCHRONIZE_ACCESS,
        )? else {
            return match Self::observe_durable(identity)? {
                ProbeObservation::ExactAbsent => Ok(()),
                ProbeObservation::ExactLive
                | ProbeObservation::Mismatched
                | ProbeObservation::Unverifiable => Err(owner_error(
                    "Windows detached owner is not exact before Job termination",
                )),
            };
        };
        if inspect_target(job.as_raw_handle().cast(), identity.target_identity())?
            != ProbeObservation::ExactLive
        {
            return Err(owner_error(
                "Windows detached owner changed before Job termination",
            ));
        }
        // SAFETY: `job` retains the exact reopened Job through this termination request.
        if unsafe { TerminateJobObject(job.as_raw_handle().cast(), 1) } == 0 {
            return Err(owner_cause(
                "Windows detached owner Job termination failed",
                std::io::Error::last_os_error(),
            ));
        }
        Ok(())
    }
}

fn install_kill_on_close(job: HANDLE) -> Result<(), ProcessError> {
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
        .map_err(|_| spawn_error("Windows Job limit record size is not representable"))?;
    // SAFETY: the live Job accepts this correctly sized immutable limits record.
    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast::<c_void>(),
            size,
        )
    } == 0
    {
        return Err(spawn_cause(
            "Windows detached owner kill-on-close policy failed",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

fn open_job(name: &str, access: u32) -> Result<Option<File>, ProcessError> {
    let wide = wide(name);
    // SAFETY: the immutable name is terminated and inheritance is disabled.
    let raw = unsafe { OpenJobObjectW(access, 0, wide.as_ptr()) };
    if raw.is_null() {
        // SAFETY: sampled immediately after the failed open call on this thread.
        return if unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND {
            Ok(None)
        } else {
            Err(owner_cause(
                "Windows detached owner Job Object cannot be reopened",
                std::io::Error::last_os_error(),
            ))
        };
    }
    // SAFETY: the new non-null handle transfers into exactly one File owner.
    Ok(Some(unsafe { File::from_raw_handle(raw.cast()) }))
}

fn inspect_target(
    job: HANDLE,
    identity: ProcessTreeIdentity,
) -> Result<ProbeObservation, ProcessError> {
    let Some(expected_start) = identity.start_token() else {
        return Ok(ProbeObservation::Unverifiable);
    };
    // SAFETY: inheritance is disabled and the PID is passed by value.
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE_ACCESS,
            0,
            identity.root_pid(),
        )
    };
    if raw.is_null() {
        // SAFETY: sampled immediately after the failed open call on this thread.
        return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
            Ok(ProbeObservation::ExactAbsent)
        } else {
            Err(owner_cause(
                "Windows detached owner process cannot be opened",
                std::io::Error::last_os_error(),
            ))
        };
    }
    // SAFETY: the new non-null handle transfers into exactly one File owner.
    let process = unsafe { File::from_raw_handle(raw.cast()) };
    let observed_start = process_start_token(&process).ok_or_else(|| {
        owner_cause(
            "Windows detached owner birth identity cannot be observed",
            std::io::Error::last_os_error(),
        )
    })?;
    if observed_start != expected_start {
        return Ok(ProbeObservation::Mismatched);
    }
    // SAFETY: the retained process handle has SYNCHRONIZE access.
    match unsafe { WaitForSingleObject(process.as_raw_handle().cast(), 0) } {
        WAIT_OBJECT_0 => return Ok(ProbeObservation::ExactAbsent),
        WAIT_TIMEOUT => {}
        _ => {
            return Err(owner_cause(
                "Windows detached owner liveness cannot be observed",
                std::io::Error::last_os_error(),
            ));
        }
    }
    let mut member = 0;
    // SAFETY: both handles remain live and `member` is writable for this call.
    if unsafe {
        IsProcessInJob(
            process.as_raw_handle().cast(),
            job,
            &raw mut member,
        )
    } == 0
    {
        return Err(owner_cause(
            "Windows detached owner Job membership cannot be observed",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(if member == 0 {
        ProbeObservation::Unverifiable
    } else {
        ProbeObservation::ExactLive
    })
}

fn process_start_token(process: &File) -> Option<u64> {
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: each pointer names writable FILETIME storage and the query handle remains live.
    let observed = unsafe {
        GetProcessTimes(
            process.as_raw_handle().cast(),
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    (observed != 0)
        .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

fn active_processes(job: HANDLE) -> Result<u32, ProcessError> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    let size = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
        .map_err(|_| owner_error("Windows Job accounting size is not representable"))?;
    let mut returned = 0_u32;
    // SAFETY: the Job is live and the complete output record is writable.
    if unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicAccountingInformation,
            (&raw mut accounting).cast::<c_void>(),
            size,
            &raw mut returned,
        )
    } == 0
        || returned != size
    {
        return Err(owner_cause(
            "Windows detached owner Job accounting cannot be observed",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(accounting.ActiveProcesses)
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

const fn spawn_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Supervisor,
        ProcessOperation::Spawn,
        RecoveryClass::RetryPreparation,
        detail,
    )
}

fn spawn_cause(detail: &'static str, cause: std::io::Error) -> ProcessError {
    ProcessError::with_source(
        ErrorCode::Supervisor,
        ProcessOperation::Spawn,
        RecoveryClass::RetryPreparation,
        detail,
        cause,
    )
}

const fn owner_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Indeterminate,
        ProcessOperation::Reconcile,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}

fn owner_cause(detail: &'static str, cause: std::io::Error) -> ProcessError {
    ProcessError::with_source(
        ErrorCode::Indeterminate,
        ProcessOperation::Reconcile,
        RecoveryClass::ReopenAndReconcile,
        detail,
        cause,
    )
}
