//! Platform-specific durable filesystem operations.

use std::{io, path::Path};

#[cfg(not(windows))]
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;

pub fn sync_directory(directory: &Path) -> io::Result<()> {
    sync_directory_os(directory)
}

#[cfg(not(windows))]
fn sync_directory_os(directory: &Path) -> io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(windows)]
fn sync_directory_os(directory: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(directory)?
        .sync_all()
}
