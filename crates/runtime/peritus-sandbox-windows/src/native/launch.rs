//! Literal suspended target creation, Job Object assignment, and exact exit observation.

use core::{ffi::c_void, mem::size_of, ptr};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
        JobObjects::AssignProcessToJobObject,
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, GetProcessTimes,
            PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
            WaitForSingleObject,
        },
    },
};

use crate::{
    EnvironmentEntry, HelperManifest, TerminalMapping, WindowsError, WindowsErrorKind,
    WindowsOperation, WindowsRecovery,
};
use peritus_process::ProcessTreeIdentity;

use super::{Activation, handle::AttributeList};

pub(super) fn launch_and_wait(
    manifest: &HelperManifest,
    activation: &mut Activation,
) -> Result<i32, WindowsError> {
    launch_and_wait_inner(manifest, activation, None)
}

pub(super) fn launch_and_wait_with_channels(
    manifest: &HelperManifest,
    activation: &mut Activation,
    channels: &mut peritus_process::NativeWindowsHelperAttachment,
) -> Result<i32, WindowsError> {
    launch_and_wait_inner(manifest, activation, Some(channels))
}

fn launch_and_wait_inner(
    manifest: &HelperManifest,
    activation: &mut Activation,
    mut channels: Option<&mut peritus_process::NativeWindowsHelperAttachment>,
) -> Result<i32, WindowsError> {
    validate_native_admission(manifest, activation.secrets.environment())?;
    let channels = channels.as_deref_mut().ok_or_else(|| {
        launch_error("independent Windows containment adoption channel is unavailable")
    })?;
    let attributes =
        AttributeList::create(manifest, activation.app_container.as_ref(), &activation.terminal)?;
    let application = wide_nul(manifest.executable());
    let mut command_line = command_line(manifest.executable(), manifest.arguments());
    let directory = wide_nul(manifest.working_directory().as_os_str());
    let mut environment =
        environment_block(manifest.environment(), activation.secrets.environment());
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = u32::try_from(size_of::<STARTUPINFOEXW>())
        .map_err(|_| launch_error("extended startup record size overflowed"))?;
    startup.lpAttributeList = attributes.pointer();
    if matches!(manifest.terminal(), TerminalMapping::Pipes { .. }) {
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        // SAFETY: the helper protocol established valid standard pipe handles before activation.
        unsafe {
            startup.StartupInfo.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
            startup.StartupInfo.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
            startup.StartupInfo.hStdError = GetStdHandle(STD_ERROR_HANDLE);
        }
    }
    let mut process = PROCESS_INFORMATION::default();
    let mut flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT;
    if matches!(manifest.terminal(), TerminalMapping::Pipes { .. }) {
        flags |= CREATE_NO_WINDOW;
    }
    // SAFETY: every pointer names live, correctly encoded storage; mutable command/environment
    // buffers satisfy CreateProcessAsUserW and the process starts suspended before job assignment.
    let created = unsafe {
        CreateProcessAsUserW(
            activation.token.raw(),
            application.as_ptr(),
            command_line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            flags,
            environment.as_mut_ptr().cast::<c_void>(),
            directory.as_ptr(),
            (&raw const startup.StartupInfo),
            &raw mut process,
        )
    };
    if created == 0 {
        return Err(launch_error("restricted literal target cannot be created"));
    }
    let mut process_handle = OwnedHandle::target(process.hProcess);
    let thread_handle = OwnedHandle::ordinary(process.hThread);
    // SAFETY: both handles are valid and the target remains suspended.
    if unsafe { AssignProcessToJobObject(activation.job.raw(), process_handle.raw()) } == 0 {
        // SAFETY: termination is confined to the just-created suspended target.
        unsafe { TerminateProcess(process_handle.raw(), 127) };
        return Err(launch_error("suspended target cannot be assigned to the exact Job Object"));
    }
    let target = target_identity(process.dwProcessId, process_handle.raw())?;
    let job_identity = crate::identity::job(
        manifest.process_id(),
        manifest.preparation_digest(),
        manifest.job(),
    );
    let adoption = peritus_process::native_target_adoption_record(
        manifest.digest(),
        manifest.preparation_digest(),
        job_identity,
    );
    channels
        .signal_target_adoption(adoption.into_bytes(), target)
        .and_then(|()| channels.await_target_adoption())
        .map_err(|_| launch_error("C2 could not adopt the suspended Windows target"))?;
    let control = channels
        .take_control_reader()
        .ok_or_else(|| launch_error("Windows terminal control ownership is unavailable"))?;
    activation.terminal.start_io(control)?;
    let execution = (|| {
        // SAFETY: the primary thread handle is live and has not been resumed.
        if unsafe { ResumeThread(thread_handle.raw()) } == u32::MAX {
            return Err(launch_error("assigned target primary thread cannot be resumed"));
        }
        let record = peritus_process::native_target_started_record(
            manifest.digest(),
            manifest.preparation_digest(),
        );
        channels
            .signal_started(record.into_bytes())
            .map_err(|_| launch_error("target-started status cannot be acknowledged"))?;
        loop {
            // This is a completion poll, not a lifetime deadline. It keeps worker failure visible
            // while retaining the target process handle as the sole completion authority.
            match unsafe { WaitForSingleObject(process_handle.raw(), 10) } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => activation.terminal.poll_io()?,
                _ => return Err(launch_error("owned target completion cannot be observed")),
            }
        }
        activation.terminal.poll_io()?;
        let mut code = 0_u32;
        // SAFETY: the completed process handle and exit-code storage are valid.
        if unsafe { GetExitCodeProcess(process_handle.raw(), &raw mut code) } == 0 {
            return Err(launch_error("owned target exit status cannot be read"));
        }
        process_handle.disarm();
        Ok(i32::try_from(code).unwrap_or(i32::MAX))
    })();
    if execution.is_err() {
        // SAFETY: termination is confined to the exact retained target before worker joins.
        unsafe { TerminateProcess(process_handle.raw(), 127) };
    }
    let workers = activation.terminal.finish_io();
    drop(attributes);
    match (execution, workers) {
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(code), Ok(())) => Ok(code),
    }
}

struct OwnedHandle {
    raw: HANDLE,
    terminate_on_drop: bool,
}

impl OwnedHandle {
    const fn target(raw: HANDLE) -> Self {
        Self { raw, terminate_on_drop: true }
    }

    const fn ordinary(raw: HANDLE) -> Self {
        Self { raw, terminate_on_drop: false }
    }

    const fn raw(&self) -> HANDLE {
        self.raw
    }

    const fn disarm(&mut self) {
        self.terminate_on_drop = false;
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.terminate_on_drop {
            // SAFETY: this guard owns the exact target handle and fails closed before release.
            unsafe { TerminateProcess(self.raw, 127) };
        }
        // SAFETY: this wrapper uniquely owns one non-null process or thread handle.
        unsafe { CloseHandle(self.raw) };
    }
}

fn target_identity(pid: u32, process: HANDLE) -> Result<ProcessTreeIdentity, WindowsError> {
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: the exact suspended target handle and writable FILETIME records remain live.
    if pid == 0
        || unsafe {
            GetProcessTimes(
                process,
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
            )
        } == 0
    {
        return Err(launch_error("suspended target birth identity cannot be observed"));
    }
    let start = (u64::from(creation.dwHighDateTime) << 32)
        | u64::from(creation.dwLowDateTime);
    if start == 0 {
        return Err(launch_error("suspended target birth identity is zero"));
    }
    Ok(ProcessTreeIdentity::new(pid, Some(start), None, true))
}

fn environment_block(ordinary: &[EnvironmentEntry], secrets: &[EnvironmentEntry]) -> Vec<u16> {
    let mut values = ordinary.iter().chain(secrets).collect::<Vec<_>>();
    values.sort_by(|left, right| crate::manifest::windows_name_cmp(left.name(), right.name()));
    let mut block = Vec::new();
    for value in values {
        block.extend(value.name().encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.value().encode_wide());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

fn command_line(executable: &OsStr, arguments: &[std::ffi::OsString]) -> Vec<u16> {
    let mut text = quote_argument(executable);
    for argument in arguments {
        text.push(u16::from(b' '));
        text.extend(quote_argument(argument));
    }
    text.push(0);
    text
}

fn quote_argument(value: &OsStr) -> Vec<u16> {
    let units = value.encode_wide().collect::<Vec<_>>();
    if !units.is_empty()
        && !units
            .iter()
            .any(|unit| matches!(*unit, 0x20 | 0x09 | 0x22))
    {
        return units;
    }
    let mut result = vec![u16::from(b'"')];
    let mut backslashes = 0_usize;
    for unit in units {
        if unit == u16::from(b'\\') {
            backslashes += 1;
        } else {
            if unit == u16::from(b'"') {
                result.extend(core::iter::repeat_n(u16::from(b'\\'), backslashes * 2 + 1));
            } else {
                result.extend(core::iter::repeat_n(u16::from(b'\\'), backslashes));
            }
            backslashes = 0;
            result.push(unit);
        }
    }
    result.extend(core::iter::repeat_n(u16::from(b'\\'), backslashes * 2));
    result.push(u16::from(b'"'));
    result
}

fn wide_nul(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(core::iter::once(0)).collect()
}

fn validate_native_admission(
    manifest: &HelperManifest,
    secrets: &[EnvironmentEntry],
) -> Result<(), WindowsError> {
    let command = peritus_process::CommandSpec::new(
        manifest.executable().to_owned(),
        manifest.arguments().to_vec(),
    )
    .map_err(|_| launch_error("target command failed native helper validation"))?;
    let variables = manifest
        .environment()
        .iter()
        .chain(secrets)
        .map(|entry| {
            peritus_process::EnvironmentVariable::new(
                entry.name().to_owned(),
                entry.value().to_owned(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| launch_error("target environment failed native helper validation"))?;
    let environment = peritus_process::EnvironmentPlan::cleared(variables)
        .map_err(|_| launch_error("target environment failed native helper validation"))?;
    peritus_process::validate_native_command_environment(&command, &environment)
        .map_err(|_| launch_error("target exceeds current native process capacity"))
}

fn launch_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::SandboxDenied,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}
