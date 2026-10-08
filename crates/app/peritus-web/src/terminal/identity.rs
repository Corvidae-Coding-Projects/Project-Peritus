//! Operating-system process birth and process-group observations for console records.

use super::record::ProcessBinding;
use crate::error::{Result, problem};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};

#[cfg(unix)]
#[allow(unsafe_code, reason = "signal zero is the narrow process-group existence boundary")]
pub(super) fn process_group_absent(binding: &ProcessBinding) -> Result<bool> {
    let group = binding
        .process_group
        .and_then(|group| i32::try_from(group).ok())
        .filter(|group| *group > 0 && u32::try_from(*group).ok() == Some(binding.pid))
        .ok_or_else(|| problem("Console process-group binding is invalid"))?;
    // SAFETY: negating a checked positive root-led process-group identity selects only that group;
    // signal zero observes existence without delivering a signal.
    if unsafe { libc::kill(-group, 0) } == 0 {
        return Ok(false);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Ok(true),
        Some(libc::EPERM) => Ok(false),
        _ => Err(problem("Console process-group existence cannot be observed")),
    }
}

#[cfg(unix)]
#[allow(unsafe_code, reason = "negative-pid kill is the narrow console process-group signal boundary")]
pub(super) fn signal_group(binding: &ProcessBinding, signal: i32) -> Result<()> {
    let group = binding
        .process_group
        .and_then(|group| i32::try_from(group).ok())
        .filter(|group| *group > 0 && u32::try_from(*group).ok() == Some(binding.pid))
        .ok_or_else(|| problem("Console process-group binding is invalid"))?;
    // SAFETY: negating a checked positive root-led process-group identity selects only that group.
    if unsafe { libc::kill(-group, signal) } == 0 {
        return Ok(());
    }
    Err(problem("Console process-group signal failed"))
}

#[cfg(unix)]
pub(super) fn child_binding(pid: u32) -> Result<ProcessBinding> {
    let start_token = current_start_token(pid)
        .ok_or_else(|| problem("PTY child birth identity is unavailable"))?;
    let group = process_group(pid).ok_or_else(|| problem("PTY child process group is unavailable"))?;
    if group != pid {
        return Err(problem("PTY child is not the leader of its isolated process group"));
    }
    Ok(ProcessBinding {
        pid,
        start_token,
        process_group: Some(group),
        job: None,
        complete_containment: true,
    })
}

#[cfg(windows)]
pub(super) fn child_binding(pid: u32, job: &WindowsJob) -> Result<ProcessBinding> {
    if !job.contains(pid)? {
        return Err(problem("PTY child escaped its exact Windows Job Object"));
    }
    Ok(ProcessBinding {
        pid,
        start_token: current_start_token(pid)
            .ok_or_else(|| problem("PTY child birth identity is unavailable"))?,
        process_group: None,
        job: Some(job.name.clone()),
        complete_containment: true,
    })
}

#[cfg(windows)]
pub(super) struct WindowsJob {
    handle: OwnedHandle,
    pub(super) name: String,
}

#[cfg(windows)]
impl WindowsJob {
    #[allow(unsafe_code, reason = "named kill-on-close Job Object is the exact console tree boundary")]
    pub(super) fn create(token: &str) -> Result<Self> {
        use core::{ffi::c_void, mem::size_of, ptr};
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::{
            Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
            System::{
                JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                    JobObjectExtendedLimitInformation, SetInformationJobObject,
                },
                Threading::GetCurrentProcess,
            },
        };
        let name = format!("Local\\PeritusConsole-{token}");
        let wide = std::ffi::OsStr::new(&name)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: null security attributes and a terminated immutable name are valid inputs.
        let handle = unsafe { CreateJobObjectW(ptr::null(), wide.as_ptr()) };
        if handle.is_null() {
            return Err(problem("The console Job Object could not be created"));
        }
        // SAFETY: sampled immediately after the successful named-object creation call above.
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        // SAFETY: CreateJobObjectW returned a new non-null owned handle above.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        let job = Self { handle, name };
        if already_exists {
            return Err(problem("The console Job Object identity already exists"));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let bytes = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(problem)?;
        // SAFETY: the owned job and exact immutable limit structure remain live for this call.
        if unsafe {
            SetInformationJobObject(
                job.handle.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                bytes,
            )
        } == 0
        {
            return Err(problem("Kill-on-close could not be installed on the console Job Object"));
        }
        // SAFETY: GetCurrentProcess returns the current pseudo-handle and the job is live.
        if unsafe { AssignProcessToJobObject(job.handle.as_raw_handle(), GetCurrentProcess()) } == 0 {
            return Err(problem(
                "The console owner could not break away into its exact Job Object",
            ));
        }
        if !job.contains(std::process::id())? {
            return Err(problem("The console owner Job Object assignment is not observable"));
        }
        Ok(job)
    }

    #[allow(unsafe_code, reason = "Job membership is the native Windows containment observation")]
    pub(super) fn contains(&self, pid: u32) -> Result<bool> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: inheritance is disabled and pid is passed by value.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Ok(false);
        }
        // SAFETY: OpenProcess returned a new non-null owned process handle above.
        let process = unsafe { OwnedHandle::from_raw_handle(process) };
        self.contains_process(&process)
    }

    #[allow(unsafe_code, reason = "Job membership is checked against one retained process object")]
    fn contains_process(&self, process: &OwnedHandle) -> Result<bool> {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        let mut member = 0;
        // SAFETY: both handles are live and member names writable BOOL storage.
        let observed = unsafe {
            IsProcessInJob(
                process.as_raw_handle(),
                self.handle.as_raw_handle(),
                &raw mut member,
            )
        };
        if observed == 0 {
            return Err(problem("Console Job Object membership cannot be observed"));
        }
        Ok(member != 0)
    }

    #[allow(unsafe_code, reason = "Job termination is the native exact-tree termination boundary")]
    pub(super) fn terminate(&self) -> Result<()> {
        // SAFETY: the handle is a live Job Object owned by this value.
        if unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(
                self.handle.as_raw_handle(),
                1,
            )
        } == 0
        {
            return Err(problem("Console Job Object termination failed"));
        }
        Ok(())
    }

    #[allow(unsafe_code, reason = "named Job Object recovery uses the exact retained native identity")]
    pub(super) fn open(name: &str) -> Result<Self> {
        Self::open_optional(name)?
            .ok_or_else(|| problem("The exact console Job Object is unavailable"))
    }

    #[allow(unsafe_code, reason = "named Job Object recovery uses the exact retained native identity")]
    pub(super) fn open_optional(name: &str) -> Result<Option<Self>> {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, GetLastError};
        let wide = std::ffi::OsStr::new(name)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        const JOB_OBJECT_QUERY: u32 = 0x0004;
        const JOB_OBJECT_TERMINATE: u32 = 0x0008;
        // SAFETY: the immutable name is terminated and handle inheritance is disabled.
        let handle = unsafe {
            windows_sys::Win32::System::JobObjects::OpenJobObjectW(
                JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE,
                0,
                wide.as_ptr(),
            )
        };
        if handle.is_null() {
            // SAFETY: sampled immediately after the failed open call above.
            if unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            return Err(problem("The exact console Job Object cannot be opened"));
        }
        // SAFETY: OpenJobObjectW returned a new non-null owned handle above.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        Ok(Some(Self { handle, name: name.to_owned() }))
    }

    #[allow(unsafe_code, reason = "Job accounting is the native Windows tree-quiescence observation")]
    pub(super) fn active_processes(&self) -> Result<u32> {
        use core::{ffi::c_void, mem::size_of};
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            QueryInformationJobObject,
        };
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let bytes = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
            .map_err(problem)?;
        // SAFETY: the job is live and the exact mutable accounting structure is correctly sized.
        if unsafe {
            QueryInformationJobObject(
                self.handle.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast::<c_void>(),
                bytes,
                core::ptr::null_mut(),
            )
        } == 0
        {
            return Err(problem("Console Job Object accounting cannot be observed"));
        }
        Ok(accounting.ActiveProcesses)
    }
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "AttachConsole and GenerateConsoleCtrlEvent are the native Ctrl-C boundary independent of a blocked ConPTY input pipe")]
pub(super) fn interrupt_child(binding: &ProcessBinding, job: &WindowsJob) -> Result<()> {
    use windows_sys::Win32::System::Console::{CTRL_C_EVENT, GenerateConsoleCtrlEvent};

    if binding.pid == 0
        || binding.start_token == 0
        || binding.job.as_deref() != Some(job.name.as_str())
        || !binding.complete_containment
    {
        return Err(problem(
            "Console process identity cannot be verified for interruption",
        ));
    }
    let process = WindowsProcessProof::verify(binding, job)?;
    let mut attachment = ConsoleAttachment::attach(binding.pid)?;
    // AttachConsole addresses a PID rather than a process handle. Recheck the live birth and job
    // membership after attachment so PID exit/reuse between verification and attachment fails
    // closed before any console-wide event is generated. Once attached, the console association
    // itself cannot switch to a replacement process.
    if !process.still_matches(binding, job)? {
        attachment.detach()?;
        return Err(problem(
            "Console process identity changed while attaching for interruption",
        ));
    }
    // SAFETY: the owner is attached to the exact verified child console and ignores Ctrl-C itself;
    // process group zero targets every process sharing only that console.
    let generated = unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) };
    attachment.detach()?;
    if generated == 0 {
        return Err(problem("The Windows console Ctrl-C event could not be generated"));
    }
    Ok(())
}

#[cfg(windows)]
struct WindowsProcessProof {
    handle: OwnedHandle,
    pid: u32,
    start_token: u64,
}

#[cfg(windows)]
impl WindowsProcessProof {
    #[allow(unsafe_code, reason = "one owned process handle binds birth, liveness, and Job membership observations")]
    fn verify(binding: &ProcessBinding, job: &WindowsJob) -> Result<Self> {
        use windows_sys::Win32::{
            Storage::FileSystem::SYNCHRONIZE,
            System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        };

        // SAFETY: inheritance is disabled; query and synchronize rights are the minimum needed
        // to bind creation time, liveness, and Job membership to one process object.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                0,
                binding.pid,
            )
        };
        if handle.is_null() {
            return Err(problem("Console child process cannot be opened for interruption"));
        }
        // SAFETY: OpenProcess returned a new non-null owned process handle above.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        let start_token = process_start_token(&handle)
            .ok_or_else(|| problem("Console child birth identity cannot be observed"))?;
        let process = Self { handle, pid: binding.pid, start_token };
        if process.start_token != binding.start_token
            || !job.contains_process(&process.handle)?
            || !process.is_running()?
        {
            return Err(problem(
                "Console process identity cannot be verified for interruption",
            ));
        }
        Ok(process)
    }

    fn still_matches(&self, binding: &ProcessBinding, job: &WindowsJob) -> Result<bool> {
        Ok(self.pid == binding.pid
            && self.start_token == binding.start_token
            && self.is_running()?
            && current_start_token(self.pid) == Some(self.start_token)
            && job.contains(self.pid)?)
    }

    #[allow(unsafe_code, reason = "zero-time wait observes liveness of the retained process object")]
    fn is_running(&self) -> Result<bool> {
        use windows_sys::Win32::{
            Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::WaitForSingleObject,
        };
        // SAFETY: the handle is a live SYNCHRONIZE-capable process handle owned by this value.
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            _ => Err(problem("Console child liveness cannot be observed")),
        }
    }
}

#[cfg(windows)]
struct ConsoleAttachment {
    attached: bool,
}

#[cfg(windows)]
impl ConsoleAttachment {
    #[allow(unsafe_code, reason = "temporary attachment is required to address the exact child console")]
    fn attach(pid: u32) -> Result<Self> {
        use windows_sys::Win32::System::Console::{AttachConsole, SetConsoleCtrlHandler};

        let mut value = Self { attached: false };
        // SAFETY: pid is the exact live child identity verified by interrupt_child.
        if unsafe { AttachConsole(pid) } == 0 {
            return Err(problem("The console owner could not attach to the child console"));
        }
        value.attached = true;
        // AttachConsole resets the caller's control-handler table. Install the documented
        // per-process Ctrl-C ignore attribute only after that reset and before generating Ctrl-C.
        // SAFETY: a null handler with add=true changes only the calling process's ignore state.
        if unsafe { SetConsoleCtrlHandler(None, 1) } == 0 {
            value.detach()?;
            return Err(problem("The console owner could not ignore its generated Ctrl-C event"));
        }
        Ok(value)
    }

    #[allow(unsafe_code, reason = "the temporary console attachment and Ctrl-C ignore attribute are process-owned native state")]
    fn detach(&mut self) -> Result<()> {
        use windows_sys::Win32::System::Console::FreeConsole;

        if self.attached {
            // SAFETY: this process owns the temporary console attachment established above.
            // FreeConsole also resets its control-handler table, including the Ctrl-C ignore
            // attribute installed after AttachConsole.
            if unsafe { FreeConsole() } == 0 {
                return Err(problem("The console owner could not release the child console"));
            }
            self.attached = false;
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for ConsoleAttachment {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}

#[cfg(target_os = "linux")]
pub(super) fn current_start_token(pid: u32) -> Option<u64> {
    linux_process_fields(pid).map(|(_, start)| start)
}

#[cfg(target_os = "linux")]
fn process_group(pid: u32) -> Option<u32> {
    linux_process_fields(pid).map(|(group, _)| group)
}

#[cfg(target_os = "linux")]
fn linux_process_fields(pid: u32) -> Option<(u32, u64)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    let fields = text.get(close + 2..)?.split_ascii_whitespace().collect::<Vec<_>>();
    Some((fields.get(2)?.parse().ok()?, fields.get(19)?.parse().ok()?))
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "proc_pidinfo is the narrow console birth-identity boundary")]
pub(super) fn current_start_token(pid: u32) -> Option<u64> {
    use core::{
        ffi::c_void,
        mem::{MaybeUninit, size_of},
    };
    let pid = i32::try_from(pid).ok()?;
    let size = i32::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    let mut information = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    // SAFETY: the pointer names exact writable proc_bsdinfo storage and is read only after an
    // exact-size success result.
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
    information
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|seconds| seconds.checked_add(information.pbi_start_tvusec))
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "getpgid is the narrow console process-group observation boundary")]
fn process_group(pid: u32) -> Option<u32> {
    // SAFETY: getpgid receives only a checked positive process identifier and returns by value.
    let group = unsafe { libc::getpgid(i32::try_from(pid).ok()?) };
    u32::try_from(group).ok().filter(|group| *group != 0)
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "GetProcessTimes is the narrow console birth-identity boundary")]
pub(super) fn current_start_token(pid: u32) -> Option<u64> {
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: handle inheritance is disabled and the process identifier is passed by value.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: OpenProcess returned a new non-null owned process handle above.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    process_start_token(&handle)
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "GetProcessTimes binds birth identity to one retained process handle")]
fn process_start_token(handle: &OwnedHandle) -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::GetProcessTimes,
    };
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: the handle is query-capable and every output pointer names live FILETIME storage.
    let observed = unsafe {
        GetProcessTimes(
            handle.as_raw_handle(),
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    (observed != 0)
        .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}
