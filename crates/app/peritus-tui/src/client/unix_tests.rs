use std::{
    os::{fd::AsRawFd as _, unix::net::UnixListener},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

#[tokio::test(flavor = "current_thread")]
async fn a_full_socket_queue_does_not_block_the_client_runtime_or_its_timeout() {
    let root = tempfile::Builder::new().prefix("p63-").tempdir_in("/tmp").expect("short root");
    let path = root.path().join("daemon.sock");
    let listener = UnixListener::bind(&path).expect("listener");
    small_backlog(&listener);
    let mut queued = Vec::new();
    let mut full = false;
    for _ in 0..8 {
        if let Ok(Ok(stream)) =
            tokio::time::timeout(Duration::from_millis(50), tokio::net::UnixStream::connect(&path))
                .await
        {
            queued.push(stream);
        } else {
            full = true;
            break;
        }
    }
    assert!(full, "bounded listener queue is saturated");
    // Rescue an accidentally blocking implementation so this regression fails instead of hanging
    // the test runner. A correct asynchronous connector finishes before the watchdog intervenes.
    let rescued = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&rescued);
    let (done, finished) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if finished.recv_timeout(Duration::from_secs(2)).is_err() {
            observed.store(true, Ordering::SeqCst);
            let _ = listener.accept();
        }
    });
    let result =
        tokio::time::timeout(Duration::from_millis(100), super::connect_local(&path)).await;
    let _ = done.send(());
    watchdog.join().expect("watchdog joins");
    assert!(!rescued.load(Ordering::SeqCst), "connection blocked the executor past its timeout");
    assert!(!matches!(result, Ok(Ok(_))), "a full queue must wait or return a connect error");
    drop(queued);
}

#[allow(unsafe_code, reason = "the regression limits the queue on its own listening socket")]
fn small_backlog(listener: &UnixListener) {
    // SAFETY: listen takes integer arguments; the borrowed listener owns this live descriptor.
    assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 1) }, 0, "set bounded backlog");
}
