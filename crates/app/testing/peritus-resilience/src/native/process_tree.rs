//! Platform process-tree ownership for one persistent H1 controller.

#[cfg(unix)]
mod platform {
    use std::os::unix::process::CommandExt as _;
    use std::process::{Child, Command};

    use nix::errno::Errno;
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    use crate::{SubjectError, SubjectErrorCode};

    use super::super::subject_error;

    pub struct ProcessTree {
        group: i32,
    }

    impl ProcessTree {
        pub fn configure(command: &mut Command) {
            command.process_group(0);
        }

        pub fn attach(child: &Child, _maximum_processes: u32) -> Result<Self, SubjectError> {
            let group = i32::try_from(child.id()).map_err(|_| {
                subject_error(
                    SubjectErrorCode::Supervision,
                    "controller PID exceeds the Unix process range",
                    false,
                )
            })?;
            Ok(Self { group })
        }

        pub fn terminate(&self) -> Result<(), SubjectError> {
            signal_group(self.group, Signal::SIGKILL)
        }

        pub fn finish(&self) -> Result<(), SubjectError> {
            match kill(Pid::from_raw(-self.group), None) {
                Err(Errno::ESRCH) => return Ok(()),
                Ok(()) => {}
                Err(error) => {
                    return Err(subject_error(
                        SubjectErrorCode::Cleanup,
                        format!("inspect owned controller process group: {error}"),
                        false,
                    ));
                }
            }
            signal_group(self.group, Signal::SIGKILL)?;
            Err(subject_error(
                SubjectErrorCode::Cleanup,
                "controller exited while a descendant remained in its process group",
                false,
            ))
        }
    }

    fn signal_group(group: i32, signal: Signal) -> Result<(), SubjectError> {
        match kill(Pid::from_raw(-group), Some(signal)) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            Err(error) => Err(subject_error(
                SubjectErrorCode::Supervision,
                format!("signal owned controller process group: {error}"),
                false,
            )),
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    #![allow(
        unsafe_code,
        reason = "the narrow Windows H1 controller TCB requires audited Job Object FFI"
    )]

    use core::{ffi::c_void, mem::size_of, ptr};
    use std::os::windows::{io::AsRawHandle as _, process::CommandExt as _};
    use std::process::{Child, Command};
    use std::thread;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
            JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        },
        System::Threading::CREATE_NO_WINDOW,
    };

    use crate::{SubjectError, SubjectErrorCode};

    use super::super::subject_error;

    const PROCESS_DRAIN_DEADLINE: Duration = Duration::from_secs(2);
    const PROCESS_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

    pub struct ProcessTree {
        job: Option<HANDLE>,
    }

    // SAFETY: this type uniquely owns a Windows Job Object handle. Windows kernel handles may be
    // transferred between threads, and all access remains exclusive through `&mut self`.
    unsafe impl Send for ProcessTree {}

    impl ProcessTree {
        pub fn configure(command: &mut Command) {
            command.creation_flags(CREATE_NO_WINDOW);
        }

        pub fn attach(child: &Child, maximum_processes: u32) -> Result<Self, SubjectError> {
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
                return Err(job_error("assign native controller to Job Object"));
            }
            Ok(tree)
        }

        pub fn terminate(&mut self) -> Result<(), SubjectError> {
            self.close()
        }

        pub fn finish(&mut self) -> Result<(), SubjectError> {
            let Some(job) = self.job else {
                return Ok(());
            };
            let started = Instant::now();
            loop {
                let active_processes = active_processes(job)?;
                if active_processes == 0 {
                    self.close()?;
                    return Ok(());
                }
                if started.elapsed() >= PROCESS_DRAIN_DEADLINE {
                    self.close()?;
                    return Err(subject_error(
                        SubjectErrorCode::Cleanup,
                        format!(
                            "controller exited while {active_processes} process(es) remained in its Job Object"
                        ),
                        false,
                    ));
                }
                thread::sleep(PROCESS_DRAIN_POLL_INTERVAL);
            }
        }

        fn close(&mut self) -> Result<(), SubjectError> {
            if let Some(job) = self.job.take() {
                // SAFETY: this value uniquely owns the Job Object; close enforces kill-on-close.
                if unsafe { CloseHandle(job) } == 0 {
                    return Err(job_error("close kill-on-close Job Object"));
                }
            }
            Ok(())
        }
    }

    fn active_processes(job: HANDLE) -> Result<u32, SubjectError> {
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let length = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
            .map_err(|_| job_error("encode Job Object accounting query"))?;
        // SAFETY: the live job and correctly-sized mutable accounting record are valid.
        let queried = unsafe {
            QueryInformationJobObject(
                job,
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast::<c_void>(),
                length,
                ptr::null_mut(),
            )
        };
        if queried == 0 {
            return Err(job_error("query Job Object process accounting"));
        }
        Ok(accounting.ActiveProcesses)
    }

    impl Drop for ProcessTree {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    fn job_error(action: &str) -> SubjectError {
        subject_error(SubjectErrorCode::Supervision, format!("{action} failed"), false)
    }
}

pub(super) use platform::ProcessTree;
