//! Bounded private transport diagnostics, excluding messages and user drafts.

use std::{fs::OpenOptions, io::Write as _, path::Path};

pub(super) fn record(path: Option<&Path>, stage: &str, generation: u64, detail: &str) {
    let Some(path) = path else { return };
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let Ok(mut file) = options.open(path) else { return };
    let Ok(metadata) = file.metadata() else { return };
    if !metadata.is_file() {
        return;
    }
    if metadata.len() >= 1024 * 1024 && file.set_len(0).is_err() {
        return;
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    // One physical line per event. Transport errors contain no request payloads.
    let detail = detail.replace(['\r', '\n'], " ");
    let _ = writeln!(
        file,
        "ts_unix_ms={timestamp} pid={} generation={generation} stage={stage} {detail}",
        std::process::id()
    );
}
