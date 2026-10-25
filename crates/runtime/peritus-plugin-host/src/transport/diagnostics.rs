//! Owned, continuously drained plugin stderr with a bounded retained tail.

use std::{
    collections::VecDeque,
    fmt::Write as _,
    sync::{Arc, Mutex},
};
use tokio::{io::AsyncReadExt as _, process::ChildStderr, task::JoinHandle};

const TAIL_BYTES: usize = 16 * 1024;

#[derive(Default)]
struct Tail {
    bytes: VecDeque<u8>,
    total: u64,
    read_error: Option<String>,
}

pub(super) struct Diagnostics {
    tail: Arc<Mutex<Tail>>,
    task: Mutex<Option<JoinHandle<()>>>,
    exit: Mutex<Option<String>>,
}

impl Diagnostics {
    pub(super) fn start(mut stderr: ChildStderr) -> Self {
        let tail = Arc::new(Mutex::new(Tail::default()));
        let capture = Arc::clone(&tail);
        let task = tokio::spawn(async move {
            let mut buffer = [0_u8; 4096];
            loop {
                let count = match stderr.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(count) => count,
                    Err(error) => {
                        capture
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .read_error = Some(error.to_string());
                        break;
                    }
                };
                let mut tail = capture.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                tail.total = tail.total.saturating_add(count as u64);
                tail.bytes.extend(&buffer[..count]);
                let excess = tail.bytes.len().saturating_sub(TAIL_BYTES);
                tail.bytes.drain(..excess);
            }
        });
        Self { tail, task: Mutex::new(Some(task)), exit: Mutex::new(None) }
    }

    pub(super) fn exited(&self, status: std::process::ExitStatus) {
        *self.exit.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(status.to_string());
    }

    pub(super) async fn finish(&self) {
        let task = self.task.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        if let Some(mut task) = task
            && tokio::time::timeout(std::time::Duration::from_secs(1), &mut task).await.is_err()
        {
            task.abort();
            let _ = task.await;
            self.tail.lock().unwrap_or_else(std::sync::PoisonError::into_inner).read_error =
                Some("stderr did not close after process termination".to_owned());
        }
    }

    pub(super) fn describe(&self) -> String {
        let exit = self.exit.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        let (total, bytes, read_error) = {
            let tail = self.tail.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            (tail.total, tail.bytes.iter().copied().collect::<Vec<_>>(), tail.read_error.clone())
        };
        let mut description =
            format!("; plugin {}", exit.as_deref().unwrap_or("exit status unavailable"));
        if total > 0 {
            let text = String::from_utf8_lossy(&bytes);
            let start = text.ceil_char_boundary(text.len().saturating_sub(384));
            // HostError has a 1024-byte public contract. Keep the final causal message visible
            // even after a noisy plugin filled the retained 16 KiB ring.
            let safe = text[start..].chars().flat_map(char::escape_debug).collect::<String>();
            let start = safe.ceil_char_boundary(safe.len().saturating_sub(512));
            let _ = write!(
                description,
                "; stderr tail ({} of {} bytes): {}{}",
                bytes.len(),
                total,
                if start > 0 || text.len() > 384 { "…" } else { "" },
                &safe[start..]
            );
        }
        if let Some(error) = read_error {
            let _ = write!(description, "; stderr capture: {error}");
        }
        description
    }
}

impl Drop for Diagnostics {
    fn drop(&mut self) {
        if let Some(task) =
            self.task.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner).take()
        {
            task.abort();
        }
    }
}
