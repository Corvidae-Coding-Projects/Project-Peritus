//! Kill-on-close Job Object creation and hard resource limits.

use core::{ffi::c_void, mem::size_of, ptr};
use std::{
    fs::File,
    os::windows::io::{AsRawHandle, FromRawHandle},
};

use windows_sys::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, SetLastError},
    System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
        JOB_OBJECT_LIMIT_JOB_TIME, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject,
    },
};

use crate::{JobPlan, WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

pub(super) struct OwnedJob(File);

impl OwnedJob {
    pub(super) fn create(plan: JobPlan) -> Result<Self, WindowsError> {
        Self::create_inner(plan, None, WindowsOperation::Probe)
    }

    pub(super) fn create_bound(plan: JobPlan, name: &str) -> Result<Self, WindowsError> {
        Self::create_inner(plan, Some(name), WindowsOperation::Prepare)
    }

    fn create_inner(
        plan: JobPlan,
        name: Option<&str>,
        operation: WindowsOperation,
    ) -> Result<Self, WindowsError> {
        plan.validate_native_capacity(operation)?;
        let wide_name = name.map(|value| {
            value.encode_utf16().chain(core::iter::once(0)).collect::<Vec<_>>()
        });
        let name_pointer = wide_name.as_ref().map_or(ptr::null(), |value| value.as_ptr());
        // SAFETY: the optional terminated name remains live and default security is requested.
        unsafe { SetLastError(0) };
        let raw = unsafe { CreateJobObjectW(ptr::null(), name_pointer) };
        if raw.is_null() {
            return Err(job_error(operation, "kill-on-close Job Object cannot be created"));
        }
        // SAFETY: the non-null Job Object handle transfers into File and is closed exactly once.
        let job = Self(unsafe { File::from_raw_handle(raw.cast()) });
        if name.is_some() {
            // SAFETY: this reads the immediately preceding CreateJobObjectW result on this thread.
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                return Err(job_error(
                    operation,
                    "authority-bound Job Object name already exists",
                ));
            }
        }
        let limits = expected_limits(plan, operation)?;
        let length = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| job_error(operation, "Job Object limit record size overflowed"))?;
        // SAFETY: the job and immutable correctly-sized limit record remain live for the call.
        if unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                length,
            )
        } == 0
        {
            return Err(job_error(operation, "Job Object limits cannot be installed exactly"));
        }
        Ok(job)
    }

    pub(super) fn adopt(file: File, plan: JobPlan) -> Result<Self, WindowsError> {
        plan.validate_native_capacity(WindowsOperation::Activate)?;
        let job = Self(file);
        let expected = expected_limits(plan, WindowsOperation::Activate)?;
        let mut observed = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let length = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| job_error(WindowsOperation::Activate, "Job Object query size overflowed"))?;
        let mut returned = 0_u32;
        // SAFETY: the inherited Job handle and complete writable output record remain live.
        if unsafe {
            QueryInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&raw mut observed).cast::<c_void>(),
                length,
                &raw mut returned,
            )
        } == 0
            || returned != length
            || observed.BasicLimitInformation.LimitFlags
                != expected.BasicLimitInformation.LimitFlags
            || observed.BasicLimitInformation.ActiveProcessLimit
                != expected.BasicLimitInformation.ActiveProcessLimit
            || observed.BasicLimitInformation.PerJobUserTimeLimit
                != expected.BasicLimitInformation.PerJobUserTimeLimit
            || observed.JobMemoryLimit != expected.JobMemoryLimit
        {
            return Err(job_error(
                WindowsOperation::Activate,
                "inherited Job Object limits differ from the admitted manifest",
            ));
        }
        Ok(job)
    }

    pub(super) fn raw(&self) -> HANDLE {
        self.0.as_raw_handle().cast()
    }

    pub(super) fn into_file(self) -> File {
        self.0
    }
}

fn expected_limits(
    plan: JobPlan,
    operation: WindowsOperation,
) -> Result<JOBOBJECT_EXTENDED_LIMIT_INFORMATION, WindowsError> {
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if let Some(active_process_limit) = plan.active_process_limit_option() {
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = active_process_limit;
    }
    if let Some(job_memory_bytes) = plan.job_memory_bytes_option() {
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
        limits.JobMemoryLimit = usize::try_from(job_memory_bytes).map_err(|_| {
            job_error(operation, "job memory ceiling exceeds this Windows architecture")
        })?;
    }
    if let Some(cpu_time_millis) = plan.cpu_time_millis_option() {
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_TIME;
        limits.BasicLimitInformation.PerJobUserTimeLimit = cpu_time_millis
            .checked_mul(10_000)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| job_error(operation, "job CPU ceiling exceeds Windows representation"))?;
    }
    Ok(limits)
}

fn job_error(operation: WindowsOperation, detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Job,
        operation,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}
