//! Native regression coverage for Task Scheduler's `WM_CLOSE` boundary.

use std::{
    fs, io,
    process::ExitCode,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsWindowVisible, PostMessageW, WM_CLOSE,
};

use super::super::StopIntent;
use super::{Observer, wide};

const FIXTURE_TIMEOUT: Duration = Duration::from_secs(5);
const FIXTURE_POLL_INTERVAL: Duration = Duration::from_millis(10);
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[test]
fn wm_close_retains_the_exact_generation_and_stops_the_observer() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let marker = temporary.path().join("supervisor.stop");
    let token = unique_token("wm-close");
    let stop = Arc::new(StopIntent::new());
    let observer = Observer::start(Arc::clone(&stop), marker.clone(), token.clone())
        .expect("start native stop observer");

    let class_name = wide(&format!("PeritusSupervisor-{}-{token}", std::process::id()));
    // SAFETY: both pointers are valid for the duration of this lookup and the observer's ready
    // handshake guarantees that its top-level window has already been created.
    let window = unsafe { FindWindowW(class_name.as_ptr(), ptr::null()) };
    assert!(!window.is_null(), "native stop window was not found by its exact class");
    // SAFETY: `window` remains owned by the live observer until WM_CLOSE is dispatched.
    assert_eq!(unsafe { IsWindowVisible(window) }, 0, "native stop window must remain hidden");

    // SAFETY: `window` is the observer-owned top-level window returned by FindWindowW.
    let posted = unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
    assert_ne!(
        posted,
        0,
        "failed to post WM_CLOSE to native stop window: {}",
        io::Error::last_os_error()
    );

    wait_for_fixture("retained WM_CLOSE stop intent", || {
        stop.requested() && fs::read(&marker).is_ok_and(|observed| observed == token.as_bytes())
    });
    assert_eq!(stop.exit_code(), ExitCode::SUCCESS);
    assert_eq!(fs::read(&marker).expect("retained stop marker"), token.as_bytes());
    shutdown_with_fixture_timeout(observer);
}

#[test]
fn observer_shutdown_does_not_require_a_queued_window_message() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let marker = temporary.path().join("supervisor.stop");
    let stop = Arc::new(StopIntent::new());
    let observer =
        Observer::start(Arc::clone(&stop), marker.clone(), unique_token("early-shutdown"))
            .expect("start native stop observer");

    shutdown_with_fixture_timeout(observer);

    assert!(!stop.requested());
    assert_eq!(
        fs::metadata(marker).expect_err("shutdown must not publish stop intent").kind(),
        io::ErrorKind::NotFound
    );
}

fn unique_token(purpose: &str) -> String {
    let sequence = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    format!("test-{purpose}-{sequence}")
}

fn wait_for_fixture(description: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + FIXTURE_TIMEOUT;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {description}");
        thread::sleep(FIXTURE_POLL_INTERVAL);
    }
}

fn shutdown_with_fixture_timeout(observer: Observer) {
    let (finished_sender, finished_receiver) = mpsc::sync_channel(1);
    let shutdown = thread::spawn(move || {
        let result = observer.shutdown();
        let _ = finished_sender.send(result);
    });
    finished_receiver
        .recv_timeout(FIXTURE_TIMEOUT)
        .expect("native observer shutdown must not require a queued message")
        .expect("native observer shutdown");
    shutdown.join().expect("join native observer shutdown fixture");
}
