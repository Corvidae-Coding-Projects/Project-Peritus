//! Platform process-tree ownership for one native probe.

#[cfg(unix)]
mod platform {
    use std::os::unix::process::CommandExt as _;
    use std::process::{Child, Command};

    use nix::errno::Errno;
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    use crate::QualificationError;

    use super::super::native_error;

    pub struct ProcessTree {
        group: i32,
    }

    impl ProcessTree {
        pub fn configure(command: &mut Command) {
            command.process_group(0);
        }

        pub fn attach(child: &Child, _maximum_processes: u32) -> Result<Self, QualificationError> {
            let group = i32::try_from(child.id()).map_err(|_| {
                native_error("own native H0 process tree", "child PID exceeds Unix process range")
            })?;
            Ok(Self { group })
        }

        pub fn terminate(&self) -> Result<(), QualificationError> {
            signal_group(self.group, Signal::SIGKILL)
        }

        pub fn finish(&self) -> Result<(), QualificationError> {
            match kill(Pid::from_raw(-self.group), None) {
                Err(Errno::ESRCH) => return Ok(()),
                Ok(()) => {}
                Err(error) => {
                    return Err(native_error(
                        "reconcile native H0 process tree",
                        format!("inspect owned process group: {error}"),
                    ));
                }
            }
            signal_group(self.group, Signal::SIGKILL)?;
            Err(native_error(
                "reconcile native H0 process tree",
                "executor exited while a descendant remained in its owned process group",
            ))
        }
    }

    fn signal_group(group: i32, signal: Signal) -> Result<(), QualificationError> {
        match kill(Pid::from_raw(-group), Some(signal)) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            Err(error) => Err(native_error(
                "terminate native H0 process tree",
                format!("signal owned process group: {error}"),
            )),
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    #![allow(
        unsafe_code,
        reason = "the narrow Windows H0 process-tree TCB requires audited Job Object FFI"
    )]

    use core::{ffi::c_void, mem::size_of, ptr, slice};
    use std::os::windows::{io::AsRawHandle as _, process::CommandExt as _};
    use std::process::{Child, Command};
    use std::thread;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_PROCESS_ID_LIST,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicProcessIdList,
            JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        },
        System::Threading::{CREATE_NO_WINDOW, OpenProcess, WaitForSingleObject},
    };

    use crate::QualificationError;

    use super::super::native_error;

    const PROCESS_DRAIN_DEADLINE: Duration = Duration::from_secs(2);
    const PROCESS_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
    const PROCESS_ID_CAPACITY: usize = 4_096;
    const PROCESS_SYNCHRONIZE: u32 = 0x0010_0000;

    pub struct ProcessTree {
        job: Option<HANDLE>,
    }

    impl ProcessTree {
        pub fn configure(command: &mut Command) {
            command.creation_flags(CREATE_NO_WINDOW);
        }

        pub fn attach(child: &Child, maximum_processes: u32) -> Result<Self, QualificationError> {
            // SAFETY: null attributes and name request an unnamed job with default security.
            let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if raw.is_null() {
                return Err(job_error("create kill-on-close Job Object"));
            }
            let tree = Self { job: Some(raw) };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            limits.BasicLimitInformation.ActiveProcessLimit = maximum_processes;
            let length = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .map_err(|_| job_error("encode Job Object limits"))?;
            // SAFETY: the job and correctly-sized immutable limit record remain live for the call.
            let installed = unsafe {
                SetInformationJobObject(
                    raw,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast::<c_void>(),
                    length,
                )
            };
            if installed == 0 {
                return Err(job_error("install Job Object limits"));
            }
            let process = child.as_raw_handle().cast::<c_void>();
            // SAFETY: the std child owns a live process handle and the job remains live in `tree`.
            if unsafe { AssignProcessToJobObject(raw, process) } == 0 {
                return Err(job_error("assign native executor to Job Object"));
            }
            Ok(tree)
        }

        pub fn terminate(&mut self) -> Result<(), QualificationError> {
            self.close()
        }

        pub fn finish(&mut self) -> Result<(), QualificationError> {
            let Some(job) = self.job else {
                return Ok(());
            };
            let started = Instant::now();
            loop {
                let running_processes = running_processes(job)?;
                if running_processes == 0 {
                    self.close()?;
                    return Ok(());
                }
                if started.elapsed() >= PROCESS_DRAIN_DEADLINE {
                    self.close()?;
                    return Err(native_error(
                        "reconcile native H0 process tree",
                        format!(
                            "executor exited while {running_processes} process(es) remained active in its Job Object"
                        ),
                    ));
                }
                thread::sleep(PROCESS_DRAIN_POLL_INTERVAL);
            }
        }

        fn close(&mut self) -> Result<(), QualificationError> {
            if let Some(job) = self.job.take() {
                // SAFETY: this value uniquely owns the Job Object; close enforces kill-on-close.
                if unsafe { CloseHandle(job) } == 0 {
                    return Err(job_error("close kill-on-close Job Object"));
                }
            }
            Ok(())
        }
    }

    fn running_processes(job: HANDLE) -> Result<u32, QualificationError> {
        let bytes = size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
            .checked_add((PROCESS_ID_CAPACITY - 1) * size_of::<usize>())
            .ok_or_else(|| job_error("size Job Object process list"))?;
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let list = storage.as_mut_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
        let length =
            u32::try_from(bytes).map_err(|_| job_error("encode Job Object process list"))?;
        // SAFETY: `storage` is usize-aligned, writable for `length` bytes, and lives through the
        // query and the subsequent bounded reads from its variable-length process-ID tail.
        let queried = unsafe {
            QueryInformationJobObject(
                job,
                JobObjectBasicProcessIdList,
                list.cast::<c_void>(),
                length,
                ptr::null_mut(),
            )
        };
        if queried == 0 {
            return Err(job_error("query Job Object process list"));
        }
        // SAFETY: the successful query initialized the fixed header in `storage`.
        let assigned = unsafe { (*list).NumberOfAssignedProcesses as usize };
        // SAFETY: the successful query initialized the fixed header in `storage`.
        let returned = unsafe { (*list).NumberOfProcessIdsInList as usize };
        if assigned > returned || returned > PROCESS_ID_CAPACITY {
            return Err(job_error("capture complete bounded Job Object process list"));
        }
        // SAFETY: the query reported at most `PROCESS_ID_CAPACITY` initialized entries in the
        // variable-length tail allocated above.
        let identifiers =
            unsafe { slice::from_raw_parts((*list).ProcessIdList.as_ptr(), returned) };
        let mut running = 0_u32;
        for identifier in identifiers {
            let process_id = u32::try_from(*identifier)
                .map_err(|_| job_error("decode Job Object process identifier"))?;
            if process_is_running(process_id)? {
                running = running.saturating_add(1);
            }
        }
        Ok(running)
    }

    fn process_is_running(process_id: u32) -> Result<bool, QualificationError> {
        // SAFETY: requests only synchronization access to the kernel-owned PID returned by the
        // Job Object query; the returned handle is closed below on every successful open.
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, process_id) };
        if process.is_null() {
            // SAFETY: immediately reads this thread's last-error value from the failed open.
            let error = unsafe { GetLastError() };
            if error == ERROR_INVALID_PARAMETER {
                return Ok(false);
            }
            return Err(job_error("open Job Object process for exit observation"));
        }
        // SAFETY: `process` is a live synchronization handle and a zero timeout does not block.
        let state = unsafe { WaitForSingleObject(process, 0) };
        // SAFETY: this function uniquely owns the handle returned by `OpenProcess`.
        if unsafe { CloseHandle(process) } == 0 {
            return Err(job_error("close Job Object process observation handle"));
        }
        match state {
            WAIT_OBJECT_0 => Ok(false),
            WAIT_TIMEOUT => Ok(true),
            _ => Err(job_error("observe Job Object process exit state")),
        }
    }

    impl Drop for ProcessTree {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    fn job_error(action: &str) -> QualificationError {
        native_error("own native H0 process tree", format!("{action} failed"))
    }
}

pub(super) use platform::ProcessTree;
