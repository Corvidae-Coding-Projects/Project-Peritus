//! Kill-on-close Job Object creation and hard resource limits.

use core::{ffi::c_void, mem::size_of, ptr};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
        JOB_OBJECT_LIMIT_JOB_TIME, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    },
};

use crate::{JobPlan, WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

pub(super) struct OwnedJob(HANDLE);

impl OwnedJob {
    pub(super) fn create(plan: JobPlan) -> Result<Self, WindowsError> {
        // SAFETY: null attributes/name request an unnamed job with default security.
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw.is_null() {
            return Err(job_error("kill-on-close Job Object cannot be created"));
        }
        let job = Self(raw);
        let memory = usize::try_from(plan.job_memory_bytes())
            .map_err(|_| job_error("job memory ceiling exceeds this Windows architecture"))?;
        let cpu_100ns = plan
            .cpu_time_millis()
            .checked_mul(10_000)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| job_error("job CPU ceiling exceeds Windows representation"))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_JOB_MEMORY
            | JOB_OBJECT_LIMIT_JOB_TIME;
        limits.BasicLimitInformation.ActiveProcessLimit = plan.active_process_limit();
        limits.BasicLimitInformation.PerJobUserTimeLimit = cpu_100ns;
        limits.JobMemoryLimit = memory;
        let length = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| job_error("Job Object limit record size overflowed"))?;
        // SAFETY: the job and immutable correctly-sized limit record remain live for the call.
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                length,
            )
        } == 0
        {
            return Err(job_error("Job Object limits cannot be installed exactly"));
        }
        Ok(job)
    }

    pub(super) const fn raw(&self) -> HANDLE {
        self.0
    }

    pub(super) fn probe() -> bool {
        JobPlan::from_manifest(true, 1, 64 * 1_024 * 1_024, 1_000).and_then(Self::create).is_ok()
    }
}

impl Drop for OwnedJob {
    fn drop(&mut self) {
        // SAFETY: this type uniquely owns the job; close enforces kill-on-close.
        unsafe { CloseHandle(self.0) };
    }
}

fn job_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Job,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}
