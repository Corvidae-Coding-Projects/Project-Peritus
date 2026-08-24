//! Process-owned anonymous handles for native proxy and secret delivery.

use core::fmt;
use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    sync::Arc,
};

use zeroize::Zeroize;

use crate::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

const MAX_LABEL_BYTES: usize = 256;
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

/// One anonymous, read-only-by-convention payload handle retained for a native helper.
///
/// The handle has no stable filesystem name. Its label and numeric operating-system identity are
/// nonsensitive manifest inputs; payload bytes are omitted from `Debug`, hashes, canonical plans,
/// argv, and environment values. Clones share one owner and the final drop truncates the backing
/// object before closing it.
#[derive(Clone)]
pub struct NativeProtectedHandle {
    label: String,
    payload_len: Option<usize>,
    inner: Arc<ProtectedHandleInner>,
}

impl NativeProtectedHandle {
    /// Creates an anonymous handle containing bounded protected bytes.
    ///
    /// The supplied allocation is zeroized after its bytes have been copied into the anonymous
    /// operating-system object. The returned raw handle remains stable until the final clone drops.
    ///
    /// # Errors
    ///
    /// Rejects empty/oversized payloads, invalid labels, or anonymous-file I/O failures.
    pub fn from_bytes(
        label: impl Into<String>,
        mut payload: Vec<u8>,
    ) -> Result<Self, ProcessError> {
        let label = label.into();
        if !valid_label(&label) {
            payload.zeroize();
            return Err(handle_error("native protected handle label is invalid"));
        }
        if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
            payload.zeroize();
            return Err(handle_error("native protected payload is empty or exceeds its bound"));
        }
        let payload_len = payload.len();
        let result = (|| {
            let mut file = tempfile::tempfile()
                .map_err(|_| handle_error("native protected anonymous handle creation failed"))?;
            file.write_all(&payload)
                .and_then(|()| file.flush())
                .and_then(|()| file.seek(SeekFrom::Start(0)).map(drop))
                .map_err(|_| handle_error("native protected anonymous handle staging failed"))?;
            Ok(Self {
                label,
                payload_len: Some(payload_len),
                inner: Arc::new(ProtectedHandleInner { file, truncate_on_drop: true }),
            })
        })();
        payload.zeroize();
        result
    }

    /// Retains a pre-opened protected operating-system object for exact child inheritance.
    ///
    /// This is used for bidirectional broker channels whose content is not a finite staged
    /// payload. The caller transfers the only parent-side ownership of `file`; clones of the
    /// returned value share its lifetime until native-session release.
    ///
    /// # Errors
    ///
    /// Rejects an invalid manifest label.
    pub fn from_file(label: impl Into<String>, file: File) -> Result<Self, ProcessError> {
        let label = label.into();
        if !valid_label(&label) {
            return Err(handle_error("native protected handle label is invalid"));
        }
        Ok(Self {
            label,
            payload_len: None,
            inner: Arc::new(ProtectedHandleInner { file, truncate_on_drop: false }),
        })
    }

    /// Returns the nonsensitive manifest label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the bounded payload length without exposing its bytes.
    #[must_use]
    pub const fn payload_len(&self) -> Option<usize> {
        self.payload_len
    }

    /// Returns the stable numeric operating-system handle used by the backend manifest.
    #[cfg(unix)]
    #[must_use]
    pub fn raw_handle(&self) -> u64 {
        use std::os::fd::AsRawFd;

        u64::try_from(self.inner.file.as_raw_fd()).unwrap_or(u64::MAX)
    }

    /// Returns the stable numeric operating-system handle used by the backend manifest.
    #[cfg(windows)]
    #[must_use]
    pub fn raw_handle(&self) -> u64 {
        use std::os::windows::io::AsRawHandle;

        self.inner.file.as_raw_handle() as usize as u64
    }
}

impl fmt::Debug for NativeProtectedHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProtectedHandle")
            .field("label", &self.label)
            .field("payload_len", &self.payload_len)
            .field("payload", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

struct ProtectedHandleInner {
    file: File,
    truncate_on_drop: bool,
}

impl Drop for ProtectedHandleInner {
    fn drop(&mut self) {
        if self.truncate_on_drop {
            let _ = self.file.set_len(0);
        }
    }
}

fn valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_LABEL_BYTES
        && label.is_ascii()
        && !label.bytes().any(|byte| byte.is_ascii_control())
}

const fn handle_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidInput,
        ProcessOperation::Spawn,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
