//! Protected-handle verification, exact launch whitelist, and `ConPTY` ownership.

use core::{ffi::c_void, mem::size_of, ptr};
use std::{
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    sync::{Arc, Mutex},
    thread,
};

use windows_sys::Win32::{
    Foundation::{GetHandleInformation, HANDLE},
    System::{
        Console::{
            COORD, ClosePseudoConsole, CreatePseudoConsole, GetStdHandle, HPCON,
            ResizePseudoConsole, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        },
        Pipes::CreatePipe,
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
    ConPty { console: HPCON, _input_reader: File, input_writer: Arc<Mutex<Option<File>>> },
}

impl TerminalAttachment {
    pub(super) fn create(mapping: TerminalMapping) -> Result<Self, WindowsError> {
        let TerminalMapping::ConPty { columns, rows, .. } = mapping else {
            return Ok(Self::Pipes);
        };
        let (input, input_writer) = conpty_input()?;
        let output = std_handle(STD_OUTPUT_HANDLE)?;
        let x = i16::try_from(columns).map_err(|_| terminal_error("ConPTY columns exceed i16"))?;
        let y = i16::try_from(rows).map_err(|_| terminal_error("ConPTY rows exceed i16"))?;
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
            input_writer: Arc::new(Mutex::new(Some(input_writer))),
        })
    }

    pub(super) fn start_io(&self, mut control: File) -> Result<(), WindowsError> {
        let Self::ConPty { console, input_writer, .. } = self else {
            drop(control);
            return Ok(());
        };
        let console = *console;
        let relay_writer = Arc::clone(input_writer);
        thread::Builder::new()
            .name("peritus-conpty-input".to_owned())
            .spawn(move || relay_input(&relay_writer))
            .map(drop)
            .map_err(|_| terminal_error("ConPTY input relay cannot be started"))?;
        let control_writer = Arc::clone(input_writer);
        thread::Builder::new()
            .name("peritus-conpty-control".to_owned())
            .spawn(move || {
                loop {
                    let mut kind = [0_u8; 1];
                    if control.read_exact(&mut kind).is_err() {
                        break;
                    }
                    match kind[0] {
                        1 => {
                            let mut size = [0_u8; 4];
                            if control.read_exact(&mut size).is_err() {
                                break;
                            }
                            let columns = u16::from_le_bytes([size[0], size[1]]);
                            let rows = u16::from_le_bytes([size[2], size[3]]);
                            let (Ok(x), Ok(y)) = (i16::try_from(columns), i16::try_from(rows))
                            else {
                                break;
                            };
                            // SAFETY: the helper owns the live HPCON while the target runs.
                            if unsafe { ResizePseudoConsole(console, COORD { X: x, Y: y }) } < 0 {
                                break;
                            }
                        }
                        2 | 3 => {
                            let byte = if kind[0] == 2 { 0x03 } else { 0x1c };
                            let Ok(mut guard) = control_writer.lock() else {
                                break;
                            };
                            let Some(writer) = guard.as_mut() else {
                                break;
                            };
                            if writer.write_all(&[byte]).and_then(|()| writer.flush()).is_err() {
                                break;
                            }
                        }
                        4 => {
                            if let Ok(mut guard) = control_writer.lock() {
                                guard.take();
                            }
                            break;
                        }
                        _ => break,
                    }
                }
            })
            .map(drop)
            .map_err(|_| terminal_error("ConPTY resize-control task cannot be started"))
    }
}

fn relay_input(writer: &Mutex<Option<File>>) {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut buffer = [0_u8; 8_192];
    while let Ok(count) = input.read(&mut buffer) {
        if count == 0 {
            break;
        }
        let Ok(mut guard) = writer.lock() else {
            break;
        };
        let Some(target) = guard.as_mut() else {
            break;
        };
        if target.write_all(&buffer[..count]).and_then(|()| target.flush()).is_err() {
            break;
        }
    }
    drop(input);
    if let Ok(mut guard) = writer.lock() {
        guard.take();
    }
}

impl Drop for TerminalAttachment {
    fn drop(&mut self) {
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
