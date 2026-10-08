//! Process-owned anonymous handles for native proxy and secret delivery.

use core::fmt;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::Arc,
};

use zeroize::{Zeroize, Zeroizing};

use crate::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

const MAX_LABEL_BYTES: usize = 256;

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
    /// Creates an anonymous handle containing finite protected bytes.
    ///
    /// The supplied allocation is zeroized after its bytes have been copied into the anonymous
    /// operating-system object. The returned raw handle remains stable until the final clone drops.
    ///
    /// # Errors
    ///
    /// Rejects an empty payload, invalid label, or anonymous-file I/O failures.
    pub fn from_bytes(
        label: impl Into<String>,
        mut payload: Vec<u8>,
    ) -> Result<Self, ProcessError> {
        let label = label.into();
        if !Self::label_is_supported(&label) {
            payload.zeroize();
            return Err(handle_error("native protected handle label is invalid"));
        }
        if payload.is_empty() {
            payload.zeroize();
            return Err(handle_error("native protected payload is empty"));
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

    /// Streams a finite protected payload into an anonymous operating-system object.
    ///
    /// This path has a fixed-size transfer buffer rather than a cumulative material allocation.
    /// `should_continue` is checked before every source read and `observe_bytes` receives the
    /// monotonically increasing byte count after every committed chunk. The returned handle owns
    /// the copied payload and truncates it on final drop.
    ///
    /// # Errors
    ///
    /// Rejects an invalid label, an empty or unrepresentable stream, caller cancellation, or any
    /// anonymous-file read/write/flush/rewind failure.
    pub fn from_reader(
        label: impl Into<String>,
        mut reader: impl Read,
        mut should_continue: impl FnMut() -> bool,
        mut observe_bytes: impl FnMut(usize),
    ) -> Result<Self, ProcessError> {
        let label = label.into();
        if !Self::label_is_supported(&label) {
            return Err(handle_error("native protected handle label is invalid"));
        }
        let mut file = tempfile::tempfile()
            .map_err(|_| handle_error("native protected anonymous handle creation failed"))?;
        let mut buffer = Zeroizing::new([0_u8; 64 * 1_024]);
        let mut payload_len = 0_usize;
        loop {
            if !should_continue() {
                return Err(stream_error("native protected payload staging was cancelled"));
            }
            let count = reader
                .read(&mut *buffer)
                .map_err(|_| handle_error("native protected payload source could not be read"))?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .map_err(|_| handle_error("native protected anonymous handle staging failed"))?;
            payload_len = payload_len.checked_add(count).ok_or_else(|| {
                handle_error("native protected payload length is not representable")
            })?;
            observe_bytes(payload_len);
        }
        if payload_len == 0 {
            return Err(handle_error("native protected payload is empty"));
        }
        file.flush()
            .and_then(|()| file.seek(SeekFrom::Start(0)).map(drop))
            .map_err(|_| handle_error("native protected anonymous handle staging failed"))?;
        Ok(Self {
            label,
            payload_len: Some(payload_len),
            inner: Arc::new(ProtectedHandleInner { file, truncate_on_drop: true }),
        })
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
        if !Self::label_is_supported(&label) {
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

    /// Reports whether a nonsensitive label can identify one protected native handle.
    #[must_use]
    pub fn label_is_supported(label: &str) -> bool {
        !label.is_empty()
            && label.len() <= MAX_LABEL_BYTES
            && label.is_ascii()
            && !label.bytes().any(|byte| byte.is_ascii_control())
    }

    /// Returns the finite payload length without exposing its bytes.
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

    #[cfg(windows)]
    pub(crate) fn windows_identity_digest(&self) -> Option<peritus_types::Sha256Digest> {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::Foundation::GetHandleInformation;

        let raw = self.inner.file.as_raw_handle().cast();
        let mut flags = 0_u32;
        // SAFETY: the File retains this handle while the query writes only `flags`.
        if unsafe { GetHandleInformation(raw, &raw mut flags) } == 0 {
            return None;
        }
        let mut bytes = Vec::from(b"PERITUS-WINDOWS-PROTECTED-HANDLE-OWNER-V1\0".as_slice());
        bytes.extend_from_slice(self.label.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&self.raw_handle().to_be_bytes());
        match self.payload_len {
            Some(length) => {
                bytes.push(1);
                bytes.extend_from_slice(&u64::try_from(length).ok()?.to_be_bytes());
            }
            None => bytes.push(0),
        }
        Some(peritus_codec::sha256(&bytes))
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

const fn handle_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidInput,
        ProcessOperation::Spawn,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

const fn stream_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Supervisor,
        ProcessOperation::Spawn,
        RecoveryClass::RetryPreparation,
        detail,
    )
}
