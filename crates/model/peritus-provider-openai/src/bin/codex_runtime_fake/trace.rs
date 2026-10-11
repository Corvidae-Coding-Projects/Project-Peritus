//! Per-installed-executable request trace.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::PathBuf;

pub(super) fn record(kind: &str) -> u64 {
    let Some(directory) =
        std::env::current_exe().ok().and_then(|path| path.parent().map(PathBuf::from))
    else {
        return 1;
    };
    let path = directory.join("trace");
    let previous = std::fs::read_to_string(&path)
        .map_or(0, |value| value.lines().filter(|entry| *entry == kind).count());
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(kind.as_bytes());
        let _ = file.write_all(b"\n");
    }
    u64::try_from(previous).unwrap_or(u64::MAX).saturating_add(1)
}

pub(super) fn executable_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_default()
}
