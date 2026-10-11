//! Crash-safe pending capture bytes retained across artifact publication retries.

use super::*;
use std::{
    fs::OpenOptions,
    io::Write as _,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const MAGIC: &[u8] = b"peritus.pending-capture.v1\0";
static TEMPORARY_NONCE: AtomicU64 = AtomicU64::new(1);

pub(super) struct CapturedImage {
    pub(super) bytes: Vec<u8>,
    pub(super) digest: Sha256Digest,
    pub(super) dimensions: (u32, u32),
    pub(super) captured_unix_millis: u64,
}

pub(super) enum CaptureAttempt {
    Image(CapturedImage),
    Failed(&'static str),
}

pub(super) enum CapturePublication {
    Captured(CompletedCapture),
    Failed(&'static str),
}

pub(super) trait CapturePublisher {
    async fn publish(
        &mut self,
        capture: &CapturedImage,
    ) -> Result<CompletedCapture, AppProtocolError>;
}

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn pending_capture_path(root: &Path, operation: ControlOperationId) -> PathBuf {
    root.join("captures").join(format!("pending-{}.capture", hex(operation.as_bytes())))
}

fn read_record(path: &Path) -> Result<Option<CapturedImage>, AppProtocolError> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(app_error(Code::Backpressure)),
    };
    let length = file.metadata().map_err(|_| app_error(Code::Backpressure))?.len();
    let capacity = usize::try_from(length).map_err(|_| app_error(Code::LimitExceeded))?;
    let mut record = Vec::new();
    record.try_reserve_exact(capacity).map_err(|_| app_error(Code::Backpressure))?;
    file.read_to_end(&mut record).map_err(|_| app_error(Code::Backpressure))?;

    let header_len = MAGIC.len() + 8 + 32 + 8;
    if record.len() < header_len || !record.starts_with(MAGIC) {
        return Err(app_error(Code::MalformedFrame));
    }
    let mut offset = MAGIC.len();
    let stamp = take_u64(&record, &mut offset)?;
    let digest_end = offset + 32;
    let digest = Sha256Digest::new(
        record[offset..digest_end].try_into().map_err(|_| app_error(Code::MalformedFrame))?,
    );
    offset = digest_end;
    let image_len = usize::try_from(take_u64(&record, &mut offset)?)
        .map_err(|_| app_error(Code::LimitExceeded))?;
    if record.len().saturating_sub(offset) != image_len {
        return Err(app_error(Code::MalformedFrame));
    }
    let validated =
        peritus_product_runner::attachment::ValidatedImage::decode_original_with_policy(
            record[offset..].to_vec(),
            peritus_product_runner::attachment::ImageDecodePolicy::default(),
        )
        .map_err(|_| app_error(Code::MalformedFrame))?;
    if validated.digest() != digest {
        return Err(app_error(Code::MalformedFrame));
    }
    let dimensions = validated.dimensions();
    let bytes = validated.into_original_bytes().ok_or_else(|| app_error(Code::MalformedFrame))?;
    Ok(Some(CapturedImage { bytes, digest, dimensions, captured_unix_millis: stamp }))
}

fn take_u64(record: &[u8], offset: &mut usize) -> Result<u64, AppProtocolError> {
    let end = offset.checked_add(8).ok_or_else(|| app_error(Code::MalformedFrame))?;
    let value = u64::from_be_bytes(
        record
            .get(*offset..end)
            .ok_or_else(|| app_error(Code::MalformedFrame))?
            .try_into()
            .map_err(|_| app_error(Code::MalformedFrame))?,
    );
    *offset = end;
    Ok(value)
}

fn persist_record(
    root: &Path,
    operation: ControlOperationId,
    capture: &CapturedImage,
) -> Result<(), AppProtocolError> {
    let path = pending_capture_path(root, operation);
    let directory = path.parent().ok_or_else(|| app_error(Code::Backpressure))?;
    fs::create_dir_all(directory).map_err(|_| app_error(Code::Backpressure))?;
    let nonce = TEMPORARY_NONCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(
        ".pending-{}-{}-{nonce}.tmp",
        std::process::id(),
        hex(operation.as_bytes()),
    ));
    let _cleanup = TemporaryFile(temporary.clone());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| app_error(Code::Backpressure))?;
    let image_len =
        u64::try_from(capture.bytes.len()).map_err(|_| app_error(Code::LimitExceeded))?;
    file.write_all(MAGIC)
        .and_then(|()| file.write_all(&capture.captured_unix_millis.to_be_bytes()))
        .and_then(|()| file.write_all(capture.digest.as_bytes()))
        .and_then(|()| file.write_all(&image_len.to_be_bytes()))
        .and_then(|()| file.write_all(&capture.bytes))
        .and_then(|()| file.sync_all())
        .map_err(|_| app_error(Code::Backpressure))?;
    drop(file);
    match fs::hard_link(&temporary, &path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = read_record(&path)?.ok_or_else(|| app_error(Code::Backpressure))?;
            if existing.bytes != capture.bytes
                || existing.digest != capture.digest
                || existing.captured_unix_millis != capture.captured_unix_millis
            {
                return Err(app_error(Code::Backpressure));
            }
            return Ok(());
        }
        Err(_) => return Err(app_error(Code::Backpressure)),
    }
    fs::remove_file(&temporary).map_err(|_| app_error(Code::Backpressure))?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| app_error(Code::Backpressure))?;
    Ok(())
}

pub(super) async fn publish_pending_capture_at(
    root: &Path,
    operation: ControlOperationId,
    capture: impl FnOnce() -> Result<CaptureAttempt, AppProtocolError> + Send + 'static,
    publisher: &mut impl CapturePublisher,
) -> Result<CapturePublication, AppProtocolError> {
    let root = root.to_path_buf();
    // Reading/persisting images and running the deadline-owned helper are blocking work. Keep
    // the request executor available while awaiting this worker; cancelled publication can
    // retry the same immutable pending record without rerunning a successful capture.
    let capture =
        tokio::task::spawn_blocking(move || -> Result<CaptureAttempt, AppProtocolError> {
            if let Some(capture) = read_record(&pending_capture_path(&root, operation))? {
                Ok(CaptureAttempt::Image(capture))
            } else {
                let capture = capture()?;
                if let CaptureAttempt::Image(image) = &capture {
                    persist_record(&root, operation, image)?;
                }
                Ok(capture)
            }
        })
        .await
        .map_err(|_| app_error(Code::Backpressure))??;
    match capture {
        CaptureAttempt::Image(capture) => {
            publisher.publish(&capture).await.map(CapturePublication::Captured)
        }
        CaptureAttempt::Failed(detail) => Ok(CapturePublication::Failed(detail)),
    }
}

pub(super) fn settle_pending_capture_at(root: &Path, operation: ControlOperationId) {
    let path = pending_capture_path(root, operation);
    let _ = fs::remove_file(path);
}
