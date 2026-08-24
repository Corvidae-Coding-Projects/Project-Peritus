//! Literal suspended target creation, Job Object assignment, and exact exit observation.

use core::{ffi::c_void, mem::size_of, ptr};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::{
        Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
        JobObjects::AssignProcessToJobObject,
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, PROCESS_INFORMATION,
            ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
            WaitForSingleObject,
        },
    },
};

use crate::{
    EnvironmentEntry, HelperManifest, TerminalMapping, WindowsError, WindowsErrorKind,
    WindowsOperation, WindowsRecovery,
};

use super::{Activation, handle::AttributeList};

pub(super) fn launch_and_wait(
    manifest: &HelperManifest,
    activation: &Activation,
) -> Result<i32, WindowsError> {
    launch_and_wait_inner(manifest, activation, None)
}

pub(super) fn launch_and_wait_with_channels(
    manifest: &HelperManifest,
    activation: &Activation,
    channels: &mut peritus_process::NativeWindowsHelperAttachment,
) -> Result<i32, WindowsError> {
    launch_and_wait_inner(manifest, activation, Some(channels))
}

fn launch_and_wait_inner(
    manifest: &HelperManifest,
    activation: &Activation,
    mut channels: Option<&mut peritus_process::NativeWindowsHelperAttachment>,
) -> Result<i32, WindowsError> {
    let attributes =
        AttributeList::create(manifest, activation.app_container.as_ref(), &activation.terminal)?;
    let application = wide_nul(manifest.executable());
    let mut command_line = command_line(manifest.executable(), manifest.arguments());
    let directory = wide_nul(manifest.working_directory().as_str());
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
    let process_handle = OwnedHandle(process.hProcess);
    let thread_handle = OwnedHandle(process.hThread);
    // SAFETY: both handles are valid and the target remains suspended.
    if unsafe { AssignProcessToJobObject(activation.job.raw(), process_handle.raw()) } == 0 {
        // SAFETY: termination is confined to the just-created suspended target.
        unsafe { TerminateProcess(process_handle.raw(), 127) };
        return Err(launch_error("suspended target cannot be assigned to the exact Job Object"));
    }
    if let Some(control) = channels.as_mut().and_then(|value| value.take_control_reader()) {
        activation.terminal.start_io(control)?;
    }
    // SAFETY: the primary thread handle is live and has not been resumed.
    if unsafe { ResumeThread(thread_handle.raw()) } == u32::MAX {
        // SAFETY: termination is confined to the owned target.
        unsafe { TerminateProcess(process_handle.raw(), 127) };
        return Err(launch_error("assigned target primary thread cannot be resumed"));
    }
    if let Some(channels) = channels {
        let record = peritus_process::native_target_started_record(
            manifest.digest(),
            manifest.preparation_digest(),
        );
        channels
            .signal_started(record.into_bytes())
            .map_err(|_| launch_error("target-started status cannot be acknowledged"))?;
    }
    // SAFETY: the process handle remains live until the wait finishes.
    if unsafe { WaitForSingleObject(process_handle.raw(), INFINITE) } != WAIT_OBJECT_0 {
        return Err(launch_error("owned target completion cannot be observed"));
    }
    let mut code = 0_u32;
    // SAFETY: the completed process handle and exit-code storage are valid.
    if unsafe { GetExitCodeProcess(process_handle.raw(), &raw mut code) } == 0 {
        return Err(launch_error("owned target exit status cannot be read"));
    }
    drop(attributes);
    Ok(i32::try_from(code).unwrap_or(i32::MAX))
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    const fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns one non-null process or thread handle.
        unsafe { CloseHandle(self.0) };
    }
}

fn environment_block(ordinary: &[EnvironmentEntry], secrets: &[EnvironmentEntry]) -> Vec<u16> {
    let mut values = ordinary.iter().chain(secrets).collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.name().to_ascii_lowercase().cmp(&right.name().to_ascii_lowercase())
    });
    let mut block = Vec::new();
    for value in values {
        block.extend(value.name().encode_utf16());
        block.push(u16::from(b'='));
        block.extend(value.value().encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

fn command_line(executable: &str, arguments: &[String]) -> Vec<u16> {
    let mut text = quote_argument(executable);
    for argument in arguments {
        text.push(' ');
        text.push_str(&quote_argument(argument));
    }
    wide_nul(&text)
}

fn quote_argument(value: &str) -> String {
    if !value.is_empty() && !value.bytes().any(|byte| matches!(byte, b' ' | b'\t' | b'"')) {
        return value.to_owned();
    }
    let mut result = String::from('"');
    let mut backslashes = 0_usize;
    for character in value.chars() {
        if character == '\\' {
            backslashes += 1;
        } else {
            if character == '"' {
                result.extend(core::iter::repeat_n('\\', backslashes * 2 + 1));
            } else {
                result.extend(core::iter::repeat_n('\\', backslashes));
            }
            backslashes = 0;
            result.push(character);
        }
    }
    result.extend(core::iter::repeat_n('\\', backslashes * 2));
    result.push('"');
    result
}

fn wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

fn launch_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::SandboxDenied,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}
