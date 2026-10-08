//! Hidden top-level window that receives Task Scheduler's cooperative `WM_CLOSE` request.

#![allow(
    unsafe_code,
    reason = "Win32 window registration and message dispatch are the Task Scheduler stop boundary"
)]

use std::{
    ffi::c_void,
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GWLP_USERDATA, GetWindowLongPtrW, MSG, PM_REMOVE, PeekMessageW, PostQuitMessage,
        RegisterClassW, SetWindowLongPtrW, TranslateMessage, UnregisterClassW, WM_CLOSE,
        WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WM_QUIT, WNDCLASSW, WS_OVERLAPPED,
    },
};

use super::StopIntent;

#[cfg(test)]
mod tests;

const MESSAGE_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(super) struct Observer {
    shutdown: Arc<AtomicBool>,
    stop: Arc<StopIntent>,
    thread: Option<JoinHandle<()>>,
}

impl Observer {
    pub(super) fn start(stop: Arc<StopIntent>, marker: PathBuf, token: String) -> io::Result<Self> {
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new().name("peritus-supervisor-window".to_owned()).spawn(
            move || {
                observer_thread(thread_stop, marker, token, thread_shutdown, ready_sender);
            },
        )?;
        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self { shutdown, stop, thread: Some(thread) }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(error) => {
                let _ = thread.join();
                Err(io::Error::other(format!(
                    "native stop observer ended before window creation: {error}"
                )))
            }
        }
    }

    pub(super) fn shutdown(mut self) -> io::Result<()> {
        self.finish()
    }

    fn finish(&mut self) -> io::Result<()> {
        self.shutdown.store(true, Ordering::Release);
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread.join().map_err(|_| {
            self.stop.fail();
            io::Error::other("native daemon stop observer thread panicked")
        })
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            super::super::write_error(&format!("failed to stop native daemon observer: {error}"));
        }
    }
}

struct WindowState {
    stop: Arc<StopIntent>,
    marker: PathBuf,
    token: String,
}

struct WindowClass {
    instance: HINSTANCE,
    name: Vec<u16>,
}

impl WindowClass {
    fn register(token: &str) -> io::Result<Self> {
        // SAFETY: a null module name requests the current executable module without ownership.
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        if instance.is_null() {
            return Err(io::Error::last_os_error());
        }
        let name = wide(&format!("PeritusSupervisor-{}-{token}", std::process::id()));
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: name.as_ptr(),
            ..WNDCLASSW::default()
        };
        // SAFETY: `class` and its nul-terminated name remain valid for the registration call.
        if unsafe { RegisterClassW(&raw const class) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { instance, name })
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: this thread registered the class with the same instance and retained name.
        unsafe {
            UnregisterClassW(self.name.as_ptr(), self.instance);
        }
    }
}

struct StopWindow {
    handle: HWND,
    _class: WindowClass,
}

impl StopWindow {
    fn create(class: WindowClass, state: &mut WindowState) -> io::Result<Self> {
        let title = wide("Peritus daemon supervisor");
        // SAFETY: the registered class and state outlive the window and its message loop. Null
        // parent and menu create an invisible top-level window, which Task Scheduler can close.
        let handle = unsafe {
            CreateWindowExW(
                0,
                class.name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                class.instance,
                ptr::from_mut(state).cast::<c_void>(),
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle, _class: class })
    }
}

impl Drop for StopWindow {
    fn drop(&mut self) {
        // SAFETY: the handle belongs to this thread. DestroyWindow simply fails if WM_CLOSE
        // already destroyed it; class cleanup still follows after this drop implementation.
        unsafe {
            DestroyWindow(self.handle);
        }
    }
}

fn observer_thread(
    stop: Arc<StopIntent>,
    marker: PathBuf,
    token: String,
    shutdown: Arc<AtomicBool>,
    ready: mpsc::SyncSender<io::Result<()>>,
) {
    let mut state = WindowState { stop: Arc::clone(&stop), marker, token };
    let class = match WindowClass::register(&state.token) {
        Ok(class) => class,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _window = match StopWindow::create(class, &mut state) {
        Ok(window) => window,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    if catch_unwind(AssertUnwindSafe(|| message_loop(&shutdown))).is_err() {
        let error = io::Error::other("native daemon stop message loop panicked");
        super::fail_stop_source(&stop, "receive native daemon stop message", &error);
    }
}

fn message_loop(shutdown: &AtomicBool) {
    let mut message = MSG::default();
    loop {
        // SAFETY: `message` is valid for output and this thread owns the window message queue.
        let available = unsafe { PeekMessageW(&raw mut message, ptr::null_mut(), 0, 0, PM_REMOVE) };
        if available != 0 {
            if message.message == WM_QUIT {
                return;
            }
            // SAFETY: `message` was populated successfully by PeekMessageW.
            unsafe {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        } else if shutdown.load(Ordering::Acquire) {
            return;
        } else {
            thread::sleep(MESSAGE_POLL_INTERVAL);
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: Windows supplies a CREATESTRUCTW for WM_NCCREATE and lpCreateParams is the
        // WindowState pointer passed to CreateWindowExW.
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 1;
    }
    // SAFETY: GWLP_USERDATA is either zero or the live WindowState set during WM_NCCREATE.
    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut WindowState };
    match message {
        WM_CLOSE => {
            if !state.is_null() {
                let retained = catch_unwind(AssertUnwindSafe(|| {
                    // SAFETY: the owning observer thread keeps WindowState alive through dispatch.
                    let state = unsafe { &*state };
                    super::retain_stop_request(
                        &state.stop,
                        &state.marker,
                        &state.token,
                        "retain native daemon stop request",
                    );
                }));
                if retained.is_err() {
                    // SAFETY: the same lifetime guarantee applies on the panic containment path.
                    unsafe { &*state }.stop.fail();
                }
            }
            // SAFETY: Windows dispatched this message to a live window on its owning thread.
            unsafe {
                DestroyWindow(window);
            }
            0
        }
        WM_DESTROY => {
            // SAFETY: the observer owns this thread's message loop.
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        WM_NCDESTROY => {
            // SAFETY: clear the non-owning pointer before default destruction completes.
            unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        _ => {
            // SAFETY: unhandled messages retain default Win32 window semantics.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
