//! Per-installed-executable request trace.

use std::fs::OpenOptions;
use std::io::Write as _;

pub(super) fn record(kind: &str) -> u64 {
    let executable =
        std::env::current_exe().unwrap_or_else(|error| fail("executable.location", &error));
    let Some(directory) = executable.parent() else {
        fail("executable.parent", &std::io::Error::from(std::io::ErrorKind::InvalidInput));
    };
    let path = directory.join("trace");
    let previous = match std::fs::read_to_string(&path) {
        Ok(value) => value.lines().filter(|entry| *entry == kind).count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => fail("trace.read", &error),
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|error| fail("trace.open", &error));
    file.write_all(format!("{kind}\n").as_bytes())
        .unwrap_or_else(|error| fail("trace.write", &error));
    u64::try_from(previous).ok().and_then(|count| count.checked_add(1)).unwrap_or_else(|| {
        fail("trace.count", &std::io::Error::from(std::io::ErrorKind::InvalidData))
    })
}

pub(super) fn executable_name() -> String {
    let executable =
        std::env::current_exe().unwrap_or_else(|error| fail("executable.location", &error));
    let Some(name) = executable.file_name().and_then(std::ffi::OsStr::to_str) else {
        fail("executable.name", &std::io::Error::from(std::io::ErrorKind::InvalidInput));
    };
    name.to_owned()
}

fn fail(stage: &str, error: &std::io::Error) -> ! {
    // Do not print Error::Display: it can contain paths or other untrusted input. Exit 91
    // identifies fixture infrastructure failure separately from the scripted auth rejection.
    eprintln!("codex-fixture {stage} {:?} os-code={:?}", error.kind(), error.raw_os_error());
    std::process::exit(91)
}
