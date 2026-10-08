//! Protected-handle verification, exact launch whitelist, and `ConPTY` ownership.

use core::{ffi::c_void, mem::size_of, ptr};
use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_NOT_FOUND, ERROR_NO_DATA, ERROR_OPERATION_ABORTED, GetHandleInformation, HANDLE,
    },
    System::{
        Console::{
            COORD, ClosePseudoConsole, CreatePseudoConsole, GetStdHandle, HPCON,
            ResizePseudoConsole, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        },
        IO::CancelSynchronousIo,
        Pipes::{CreatePipe, PIPE_NOWAIT, SetNamedPipeHandleState},
        Threading::{
            DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
            UpdateProcThreadAttribute,
        },
    },
};

use crate::{
    HelperManifest, TerminalMapping, WindowsError, WindowsErrorKind, WindowsOperation,
    WindowsRecovery,
};

pub(super) enum TerminalAttachment {
    Pipes,
    ConPty {
        console: HPCON,
        _input_reader: File,
        input_writer: Option<File>,
        workers: Option<ConPtyWorkers>,
    },
}

impl TerminalAttachment {
    pub(super) fn validate(mapping: TerminalMapping) -> Result<(), WindowsError> {
        if let TerminalMapping::ConPty { columns, rows, .. } = mapping {
            let _ = native_coordinate(columns, "ConPTY columns exceed i16")?;
            let _ = native_coordinate(rows, "ConPTY rows exceed i16")?;
        }
        Ok(())
    }

    pub(super) fn create(mapping: TerminalMapping) -> Result<Self, WindowsError> {
        Self::validate(mapping)?;
        let TerminalMapping::ConPty { columns, rows, .. } = mapping else {
            return Ok(Self::Pipes);
        };
        let (input, input_writer) = conpty_input()?;
        let output = std_handle(STD_OUTPUT_HANDLE)?;
        let x = native_coordinate(columns, "ConPTY columns exceed i16")?;
        let y = native_coordinate(rows, "ConPTY rows exceed i16")?;
        let mut console = 0;
        // SAFETY: standard pipe handles and output HPCON storage remain valid for the call.
        if unsafe {
            CreatePseudoConsole(
                COORD { X: x, Y: y },
                input.as_raw_handle().cast(),
                output,
                0,
                &raw mut console,
            )
        } < 0
        {
            return Err(terminal_error("ConPTY cannot be created from C2 standard pipes"));
        }
        Ok(Self::ConPty {
            console,
            _input_reader: input,
            input_writer: Some(input_writer),
            workers: None,
        })
    }

    pub(super) fn start_io(&mut self, control: File) -> Result<(), WindowsError> {
        let Self::ConPty { console, input_writer, workers, .. } = self else {
            drop(control);
            return Ok(());
        };
        if workers.is_some() {
            return Err(terminal_error("ConPTY workers were already started"));
        }
        let mut writer = input_writer
            .take()
            .ok_or_else(|| terminal_error("ConPTY input ownership is unavailable"))?;
        set_pipe_nowait(&writer)?;
        let control_writer = writer
            .try_clone()
            .map_err(|_| terminal_error("ConPTY control writer cannot be retained"))?;
        set_pipe_nowait(&control_writer)?;
        let state = Arc::new(ConPtyWorkerState::new());
        let input_state = Arc::clone(&state);
        let input = thread::Builder::new()
            .name("peritus-conpty-input".to_owned())
            .spawn(move || relay_input(&mut writer, &input_state))
            .map_err(|_| terminal_error("ConPTY input relay cannot be started"))?;
        state.input_thread.store(input.as_raw_handle() as usize, Ordering::Release);
        let input_thread = input.as_raw_handle() as usize;
        let control_state = Arc::clone(&state);
        let console = *console as usize;
        let control_task = thread::Builder::new()
            .name("peritus-conpty-control".to_owned())
            .spawn(move || {
                relay_control(
                    control,
                    control_writer,
                    console as HPCON,
                    &control_state,
                )
            });
        let control_task = match control_task {
            Ok(task) => task,
            Err(_) => {
                state.stopping.store(true, Ordering::Release);
                state.changed.notify_all();
                let _ = cancel_thread(input_thread);
                let _ = input.join();
                return Err(terminal_error("ConPTY resize-control task cannot be started"));
            }
        };
        state.control_thread.store(control_task.as_raw_handle() as usize, Ordering::Release);
        *workers = Some(ConPtyWorkers { state, input: Some(input), control: Some(control_task) });
        Ok(())
    }

    pub(super) fn poll_io(&mut self) -> Result<(), WindowsError> {
        let Self::ConPty { workers: Some(workers), .. } = self else {
            return Ok(());
        };
        workers.poll()
    }

    pub(super) fn finish_io(&mut self) -> Result<(), WindowsError> {
        let Self::ConPty { workers, .. } = self else {
            return Ok(());
        };
        let Some(workers) = workers.take() else {
            return Ok(());
        };
        workers.finish()
    }
}

struct ConPtyWorkers {
    state: Arc<ConPtyWorkerState>,
    input: Option<JoinHandle<Result<(), WindowsError>>>,
    control: Option<JoinHandle<Result<(), WindowsError>>>,
}

impl ConPtyWorkers {
    fn poll(&mut self) -> Result<(), WindowsError> {
        join_finished(
            &mut self.input,
            &self.state.input_thread,
            "ConPTY input relay panicked",
        )?;
        join_finished(
            &mut self.control,
            &self.state.control_thread,
            "ConPTY control worker panicked",
        )
    }

    fn finish(mut self) -> Result<(), WindowsError> {
        let mut failure = self.state.stop().err();
        for (task, detail) in [
            (&mut self.control, "ConPTY control worker panicked"),
            (&mut self.input, "ConPTY input relay panicked"),
        ] {
            if let Some(task) = task.take() {
                let result = task.join().unwrap_or_else(|_| Err(terminal_error(detail)));
                if failure.is_none() {
                    failure = result.err();
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

struct ConPtyWorkerState {
    stopping: AtomicBool,
    input_closed: AtomicBool,
    control_pending: AtomicBool,
    input_thread: AtomicUsize,
    control_thread: AtomicUsize,
    gate: Mutex<()>,
    changed: Condvar,
}

impl ConPtyWorkerState {
    fn new() -> Self {
        Self {
            stopping: AtomicBool::new(false),
            input_closed: AtomicBool::new(false),
            control_pending: AtomicBool::new(false),
            input_thread: AtomicUsize::new(0),
            control_thread: AtomicUsize::new(0),
            gate: Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    fn input_may_continue(&self) -> Result<bool, WindowsError> {
        let mut guard = self
            .gate
            .lock()
            .map_err(|_| terminal_error("ConPTY input coordination was poisoned"))?;
        while self.control_pending.load(Ordering::Acquire)
            && !self.stopping.load(Ordering::Acquire)
            && !self.input_closed.load(Ordering::Acquire)
        {
            guard = self
                .changed
                .wait(guard)
                .map_err(|_| terminal_error("ConPTY input coordination was poisoned"))?;
        }
        Ok(!self.stopping.load(Ordering::Acquire)
            && !self.input_closed.load(Ordering::Acquire))
    }

    fn close_input(&self) -> Result<(), WindowsError> {
        self.input_closed.store(true, Ordering::Release);
        self.changed.notify_all();
        cancel_thread(self.input_thread.load(Ordering::Acquire))
    }

    fn begin_control(&self) -> Result<(), WindowsError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| terminal_error("ConPTY control coordination was poisoned"))?;
        self.control_pending.store(true, Ordering::Release);
        Ok(())
    }

    fn end_control(&self) -> Result<(), WindowsError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| terminal_error("ConPTY control coordination was poisoned"))?;
        self.control_pending.store(false, Ordering::Release);
        self.changed.notify_all();
        Ok(())
    }

    fn stop(&self) -> Result<(), WindowsError> {
        self.stopping.store(true, Ordering::Release);
        self.changed.notify_all();
        let input = cancel_thread(self.input_thread.load(Ordering::Acquire));
        let control = cancel_thread(self.control_thread.load(Ordering::Acquire));
        input.and(control)
    }
}

fn join_finished(
    task: &mut Option<JoinHandle<Result<(), WindowsError>>>,
    raw: &AtomicUsize,
    panic_detail: &'static str,
) -> Result<(), WindowsError> {
    if !task.as_ref().is_some_and(JoinHandle::is_finished) {
        return Ok(());
    }
    let result = task.take()
        .expect("finished ConPTY task remains retained")
        .join()
        .unwrap_or_else(|_| Err(terminal_error(panic_detail)));
    raw.store(0, Ordering::Release);
    result
}

fn relay_input(writer: &mut File, state: &ConPtyWorkerState) -> Result<(), WindowsError> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut buffer = [0_u8; 8_192];
    loop {
        if !state.input_may_continue()? {
            return Ok(());
        }
        let count = match input.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => count,
            Err(_error)
                if state.stopping.load(Ordering::Acquire)
                    || state.input_closed.load(Ordering::Acquire) =>
            {
                return Ok(());
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) if is_cancelled(&error) => continue,
            Err(_) => return Err(terminal_error("ConPTY input source cannot be read")),
        };
        if !write_input(writer, &buffer[..count], state)? {
            return Ok(());
        }
    }
}

fn write_input(
    writer: &mut File,
    bytes: &[u8],
    state: &ConPtyWorkerState,
) -> Result<bool, WindowsError> {
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let mut guard = state
            .gate
            .lock()
            .map_err(|_| terminal_error("ConPTY input coordination was poisoned"))?;
        while state.control_pending.load(Ordering::Acquire)
            && !state.stopping.load(Ordering::Acquire)
            && !state.input_closed.load(Ordering::Acquire)
        {
            guard = state
                .changed
                .wait(guard)
                .map_err(|_| terminal_error("ConPTY input coordination was poisoned"))?;
        }
        if state.stopping.load(Ordering::Acquire)
            || state.input_closed.load(Ordering::Acquire)
        {
            return Ok(false);
        }
        let write = writer.write(&bytes[offset..]);
        drop(guard);
        match write {
            Ok(0) => thread::sleep(Duration::from_millis(1)),
            Ok(count) => offset = offset.saturating_add(count),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if would_block(&error) => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(error)
                if (state.stopping.load(Ordering::Acquire)
                    || state.input_closed.load(Ordering::Acquire))
                    && is_cancelled(&error) =>
            {
                return Ok(false);
            }
            Err(_) => return Err(terminal_error("ConPTY input cannot be relayed")),
        }
    }
    Ok(true)
}

fn relay_control(
    mut control: File,
    writer: File,
    console: HPCON,
    state: &ConPtyWorkerState,
) -> Result<(), WindowsError> {
    let mut writer = Some(writer);
    loop {
        if state.stopping.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut kind = [0_u8; 1];
        if let Err(error) = read_control(&mut control, &mut kind, state) {
            if state.stopping.load(Ordering::Acquire) {
                return Ok(());
            }
            return Err(error);
        }
        if state.stopping.load(Ordering::Acquire) {
            return Ok(());
        }
        match kind[0] {
            1 => {
                let mut size = [0_u8; 4];
                read_control(&mut control, &mut size, state)?;
                if state.stopping.load(Ordering::Acquire) {
                    return Ok(());
                }
                let x = native_coordinate(
                    u16::from_le_bytes([size[0], size[1]]),
                    "ConPTY resize columns exceed i16",
                )?;
                let y = native_coordinate(
                    u16::from_le_bytes([size[2], size[3]]),
                    "ConPTY resize rows exceed i16",
                )?;
                // SAFETY: the retained terminal owns the live HPCON until this worker joins.
                if unsafe { ResizePseudoConsole(console, COORD { X: x, Y: y }) } < 0 {
                    return Err(terminal_error("ConPTY resize failed"));
                }
            }
            2 | 3 => {
                if state.input_closed.load(Ordering::Acquire) {
                    return Err(terminal_error("ConPTY signal arrived after input closure"));
                }
                state.begin_control()?;
                let result = writer
                    .as_mut()
                    .ok_or_else(|| terminal_error("ConPTY signal writer is closed"))
                    .and_then(|writer| {
                        write_control_byte(
                            writer,
                            if kind[0] == 2 { 0x03 } else { 0x1c },
                            state,
                        )
                    });
                state.end_control()?;
                result?;
            }
            4 => {
                state.close_input()?;
                writer.take();
            }
            _ => return Err(terminal_error("ConPTY control frame is malformed")),
        }
    }
}

fn read_control(
    control: &mut File,
    bytes: &mut [u8],
    state: &ConPtyWorkerState,
) -> Result<(), WindowsError> {
    let mut offset = 0_usize;
    while offset < bytes.len() {
        match control.read(&mut bytes[offset..]) {
            Ok(0) => return Err(terminal_error("ConPTY control owner disconnected")),
            Ok(count) => offset = offset.saturating_add(count),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if is_cancelled(&error) && state.stopping.load(Ordering::Acquire) => {
                return Ok(());
            }
            Err(error) if is_cancelled(&error) && state.input_closed.load(Ordering::Acquire) => {
                continue;
            }
            Err(_) => return Err(terminal_error("ConPTY control frame cannot be read")),
        }
    }
    Ok(())
}

fn write_control_byte(
    writer: &mut File,
    byte: u8,
    state: &ConPtyWorkerState,
) -> Result<(), WindowsError> {
    loop {
        if state.stopping.load(Ordering::Acquire) {
            return Ok(());
        }
        match writer.write(&[byte]) {
            Ok(1) => return Ok(()),
            Ok(_) => thread::sleep(Duration::from_millis(1)),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if would_block(&error) => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(_) => return Err(terminal_error("ConPTY signal cannot be delivered")),
        }
    }
}

fn cancel_thread(raw: usize) -> Result<(), WindowsError> {
    if raw == 0 {
        return Ok(());
    }
    // SAFETY: each raw value belongs to a retained JoinHandle until every worker has joined.
    if unsafe { CancelSynchronousIo(raw as HANDLE) } != 0 {
        return Ok(());
    }
    if std::io::Error::last_os_error().raw_os_error().and_then(|value| u32::try_from(value).ok())
        == Some(ERROR_NOT_FOUND)
    {
        return Ok(());
    }
    Err(terminal_error("blocked ConPTY worker cannot be cancelled"))
}

fn is_cancelled(error: &std::io::Error) -> bool {
    error.raw_os_error().and_then(|value| u32::try_from(value).ok())
        == Some(ERROR_OPERATION_ABORTED)
}

fn would_block(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::WouldBlock
        || error.raw_os_error().and_then(|value| u32::try_from(value).ok())
            == Some(ERROR_NO_DATA)
}

fn set_pipe_nowait(writer: &File) -> Result<(), WindowsError> {
    let mode = PIPE_NOWAIT;
    // SAFETY: the File owns a live pipe endpoint and only its wait mode is updated.
    if unsafe {
        SetNamedPipeHandleState(
            writer.as_raw_handle().cast(),
            &raw const mode,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(terminal_error("ConPTY input pipe cannot be made nonblocking"));
    }
    Ok(())
}

fn native_coordinate(value: u16, detail: &'static str) -> Result<i16, WindowsError> {
    i16::try_from(value).map_err(|_| terminal_error(detail))
}

impl Drop for TerminalAttachment {
    fn drop(&mut self) {
        let _ = self.finish_io();
        if let Self::ConPty { console, .. } = self {
            // SAFETY: this type uniquely owns the valid HPCON returned by CreatePseudoConsole.
            unsafe { ClosePseudoConsole(*console) };
        }
    }
}

pub(super) struct AttributeList {
    storage: Vec<usize>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
    handles: Vec<HANDLE>,
    capabilities: Option<windows_sys::Win32::Security::SECURITY_CAPABILITIES>,
}

impl AttributeList {
    pub(super) fn create(
        manifest: &HelperManifest,
        app_container: Option<&crate::native::token::AppContainerSid>,
        terminal: &TerminalAttachment,
    ) -> Result<Self, WindowsError> {
        let mut handles = manifest
            .inherited_handles()
            .handles()
            .iter()
            .map(|value| *value as HANDLE)
            .collect::<Vec<_>>();
        if matches!(terminal, TerminalAttachment::Pipes) {
            handles.extend([
                std_handle(STD_INPUT_HANDLE)?,
                std_handle(STD_OUTPUT_HANDLE)?,
                std_handle(STD_ERROR_HANDLE)?,
            ]);
        }
        handles.sort_by_key(|handle| *handle as usize);
        handles.dedup();
        let count = u32::from(!handles.is_empty())
            + u32::from(app_container.is_some())
            + u32::from(matches!(terminal, TerminalAttachment::ConPty { .. }));
        let mut byte_count = 0_usize;
        // SAFETY: the documented sizing call uses a null list and writes only required size.
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &raw mut byte_count);
        }
        if byte_count == 0 {
            return Err(handle_error("process attribute-list size cannot be determined"));
        }
        let words = byte_count.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let pointer = storage.as_mut_ptr().cast::<c_void>();
        // SAFETY: storage is aligned, sized from the prior query, and lives in this value.
        if unsafe { InitializeProcThreadAttributeList(pointer, count, 0, &raw mut byte_count) } == 0
        {
            return Err(handle_error("process attribute list cannot be initialized"));
        }
        let capabilities = app_container.map(crate::native::token::AppContainerSid::capabilities);
        let mut value = Self { storage, pointer, handles, capabilities };
        value.install_handle_list()?;
        value.install_app_container()?;
        value.install_conpty(terminal)?;
        Ok(value)
    }

    pub(super) const fn pointer(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.pointer
    }

    fn install_handle_list(&mut self) -> Result<(), WindowsError> {
        if self.handles.is_empty() {
            return Ok(());
        }
        let bytes = self
            .handles
            .len()
            .checked_mul(size_of::<HANDLE>())
            .ok_or_else(|| handle_error("inherited handle-list size overflowed"))?;
        // SAFETY: the list is initialized; handle buffer is immutable and live through launch.
        if unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                usize::try_from(PROC_THREAD_ATTRIBUTE_HANDLE_LIST).unwrap_or(usize::MAX),
                self.handles.as_ptr().cast::<c_void>(),
                bytes,
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(handle_error("exact inherited handle list cannot be installed"));
        }
        Ok(())
    }

    fn install_app_container(&mut self) -> Result<(), WindowsError> {
        let Some(capabilities) = self.capabilities.as_ref() else {
            return Ok(());
        };
        // SAFETY: capabilities and derived SID remain live through process creation.
        if unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                usize::try_from(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES).unwrap_or(usize::MAX),
                ptr::from_ref(capabilities).cast::<c_void>(),
                size_of::<windows_sys::Win32::Security::SECURITY_CAPABILITIES>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(handle_error("AppContainer security capabilities cannot be installed"));
        }
        Ok(())
    }

    fn install_conpty(&mut self, terminal: &TerminalAttachment) -> Result<(), WindowsError> {
        let TerminalAttachment::ConPty { console, .. } = terminal else {
            return Ok(());
        };
        // SAFETY: HPCON storage and attribute list remain valid through process creation.
        if unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                usize::try_from(PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE).unwrap_or(usize::MAX),
                ptr::from_ref(console).cast::<c_void>(),
                size_of::<HPCON>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(terminal_error("ConPTY process attribute cannot be installed"));
        }
        Ok(())
    }
}

fn conpty_input() -> Result<(File, File), WindowsError> {
    let mut reader: HANDLE = ptr::null_mut();
    let mut writer: HANDLE = ptr::null_mut();
    // SAFETY: output pointers are valid and null attributes create non-inheritable handles.
    if unsafe { CreatePipe(&raw mut reader, &raw mut writer, ptr::null(), 0) } == 0 {
        return Err(terminal_error("ConPTY input pipe cannot be created"));
    }
    // SAFETY: the two handles are distinct successful CreatePipe results with transferred ownership.
    let reader = unsafe { File::from_raw_handle(reader.cast()) };
    // SAFETY: the writer is independently owned and transferred into File.
    let writer = unsafe { File::from_raw_handle(writer.cast()) };
    Ok((reader, writer))
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: pointer names the initialized list backed by `storage` until drop returns.
        unsafe { DeleteProcThreadAttributeList(self.pointer) };
        self.storage.clear();
    }
}

pub(super) fn verify_protected_handles(manifest: &HelperManifest) -> Result<(), WindowsError> {
    for value in manifest
        .inherited_handles()
        .handles()
        .iter()
        .copied()
        .chain(manifest.network().proxy().map(crate::ProxyRoute::routing_handle))
        .chain(manifest.secret_handles().iter().map(crate::ProtectedSecretHandle::handle))
    {
        let mut flags = 0_u32;
        // SAFETY: this queries a caller-supplied numeric handle without dereferencing memory.
        if unsafe { GetHandleInformation(value as HANDLE, &raw mut flags) } == 0 {
            return Err(handle_error("protected inherited handle is invalid in the helper"));
        }
    }
    Ok(())
}

fn std_handle(kind: u32) -> Result<HANDLE, WindowsError> {
    // SAFETY: GetStdHandle has no memory preconditions.
    let handle = unsafe { GetStdHandle(kind) };
    if handle.is_null() || handle as isize == -1 {
        Err(handle_error("required C2 standard pipe handle is unavailable"))
    } else {
        Ok(handle)
    }
}

fn handle_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Handle,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}

fn terminal_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Terminal,
        WindowsOperation::Activate,
        WindowsRecovery::SelectBackend,
        detail,
    )
}
