//! Literal command/environment projection and suspended native process creation.
use super::{
    WindowsPty, error,
    native::{Console, Handle, Job},
};
use crate::platform::{
    NativeHandshake, OutputReader, PlatformProcess, ProcessTreeIdentity, current_start_token,
};
use crate::{CommandSpec, ExecutionPlan, OutputStream, ProcessError, StdinPolicy, TerminalSize};
use std::{mem::size_of, os::windows::ffi::OsStrExt, ptr};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::{
        JobObjects::AssignProcessToJobObject,
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, ResumeThread, STARTUPINFOEXW,
            TerminateProcess, UpdateProcThreadAttribute,
        },
    },
};

#[allow(clippy::needless_pass_by_value, reason = "matches the common platform launch entrypoint")]
pub(in crate::platform) fn launch(
    plan: &ExecutionPlan,
    command: &CommandSpec,
    handshake: Option<NativeHandshake<'_>>,
    size: TerminalSize,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    if handshake.is_some() {
        return Err(error("native helper PTY requires its protected Windows channels"));
    }
    let application: Vec<u16> =
        std::ffi::OsStr::new(command.executable()).encode_wide().chain(Some(0)).collect();
    let mut line = command_line(command);
    let cwd: Vec<u16> =
        plan.working_directory().path().as_os_str().encode_wide().chain(Some(0)).collect();
    let mut environment = environment(plan);
    let job = Job::create()?;
    let (console, input, output) = Console::create(size)?;
    let signal_input = input
        .try_clone()
        .map_err(|source| error("ConPTY control input cannot be duplicated").with_source(source))?;
    let attributes = Attributes::new(&console)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb =
        u32::try_from(size_of::<STARTUPINFOEXW>()).expect("Windows structure size");
    startup.lpAttributeList = attributes.pointer;
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: all UTF-16 strings are NUL terminated, the environment is double-NUL terminated,
    // startup has its exact extended size, and attribute storage remains live until creation ends.
    // No arbitrary handles are inherited. The initial thread stays suspended until job assignment.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_mut_ptr().cast(),
            cwd.as_ptr(),
            &raw const startup.StartupInfo,
            &raw mut info,
        )
    } == 0
    {
        return Err(error("native ConPTY target creation failed")
            .with_source(std::io::Error::last_os_error()));
    }
    // SAFETY: successful CreateProcessW transfers two distinct owned handles.
    let process = unsafe { Handle::from_raw(info.hProcess) };
    // SAFETY: the initial thread is independently owned and closed after resume.
    let thread = unsafe { Handle::from_raw(info.hThread) };
    contain_and_resume(&job, &process, thread.raw())?;
    let accepts_input = matches!(plan.stdin_policy(), StdinPolicy::Bounded { .. });
    let input = accepts_input.then_some(input);
    Ok(Box::new(WindowsPty {
        identity: ProcessTreeIdentity::new(
            info.dwProcessId,
            current_start_token(info.dwProcessId),
            None,
            true,
        ),
        process,
        job,
        console: Some(console),
        closing: None,
        input,
        signal_input: accepts_input.then_some(signal_input),
        readers: vec![OutputReader { stream: OutputStream::Terminal, reader: Box::new(output) }],
    }))
}

fn contain_and_resume(job: &Job, process: &Handle, thread: HANDLE) -> Result<(), ProcessError> {
    // SAFETY: job and still-suspended child handles remain valid; no target code has run.
    if unsafe { AssignProcessToJobObject(job.raw(), process.raw()) } == 0 {
        // SAFETY: terminate the uniquely owned suspended child when containment cannot be installed.
        unsafe { TerminateProcess(process.raw(), 1) };
        return Err(error("ConPTY target cannot enter the owned job"));
    }
    // SAFETY: this is the sole initial suspended thread, resumed only after successful containment.
    if unsafe { ResumeThread(thread) } == u32::MAX {
        job.terminate()?;
        return Err(error("contained ConPTY target cannot resume"));
    }
    Ok(())
}

fn environment(plan: &ExecutionPlan) -> Vec<u16> {
    let mut variables: Vec<_> = plan.environment().variables().iter().collect();
    variables.sort_by_key(|variable| variable.name().to_uppercase());
    let mut block = Vec::new();
    for variable in variables {
        block.extend(variable.name().encode_utf16());
        block.push(u16::from(b'='));
        block.extend(variable.value().encode_utf16());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

fn command_line(command: &CommandSpec) -> Vec<u16> {
    let mut line = Vec::new();
    for argument in
        std::iter::once(command.executable()).chain(command.arguments().iter().map(String::as_str))
    {
        if !line.is_empty() {
            line.push(u16::from(b' '));
        }
        quote_argument(argument, &mut line);
    }
    line.push(0);
    line
}
fn quote_argument(argument: &str, output: &mut Vec<u16>) {
    output.push(u16::from(b'"'));
    let mut slashes = 0;
    for unit in argument.encode_utf16() {
        if unit == u16::from(b'\\') {
            slashes += 1;
            continue;
        }
        let count = if unit == u16::from(b'"') { slashes * 2 + 1 } else { slashes };
        output.extend(std::iter::repeat_n(u16::from(b'\\'), count));
        output.push(unit);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2));
    output.push(u16::from(b'"'));
}

struct Attributes {
    _storage: Vec<usize>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}
impl Attributes {
    fn new(console: &Console) -> Result<Self, ProcessError> {
        let mut bytes = 0;
        // SAFETY: documented sizing query writes only required storage size.
        unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &raw mut bytes) };
        if bytes == 0 {
            return Err(error("ConPTY attribute sizing failed"));
        }
        let mut storage = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
        let pointer = storage.as_mut_ptr().cast();
        // SAFETY: aligned storage is allocated at the queried size and stays alive with the list.
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &raw mut bytes) } == 0 {
            return Err(error("ConPTY attribute initialization failed"));
        }
        let result = Self { _storage: storage, pointer };
        // SAFETY: pseudoconsole attribute takes the HPCON value itself, not a pointer to it.
        // The console outlives the list and the following CreateProcessW call.
        if unsafe {
            UpdateProcThreadAttribute(
                result.pointer,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                console.0 as *const core::ffi::c_void,
                size_of_val(&console.0),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(error("ConPTY process attribute failed"));
        }
        Ok(result)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: pointer references exactly the initialized list backed by live owned storage.
        unsafe { DeleteProcThreadAttributeList(self.pointer) };
    }
}
