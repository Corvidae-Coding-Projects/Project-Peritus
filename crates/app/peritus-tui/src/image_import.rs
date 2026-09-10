//! Explicit local image reads. No shell expansion, clipboard polling, or provider operations.

use peritus_app_protocol::{MAX_WORKBENCH_IMAGE_BYTES, WorkbenchImageLabel};
use peritus_types::Sha256Digest;
use std::{
    fmt,
    fs::{File, OpenOptions},
    io::Read as _,
    path::{Component, Path},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadStep {
    Begin,
    Chunk { end: usize },
    Complete,
}

pub struct ImageBytes {
    pub(super) bytes: Vec<u8>,
    pub(super) digest: Sha256Digest,
    pub(super) label: WorkbenchImageLabel,
}
impl fmt::Debug for ImageBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageBytes")
            .field("size", &self.bytes.len())
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}

pub fn read(path: &Path) -> Result<ImageBytes, &'static str> {
    validate_explicit_path(path)?;
    let label = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or("The file name must be valid UTF-8.")?;
    let label = WorkbenchImageLabel::new(label.to_owned())
        .map_err(|_| "The file name is empty, too long, or contains terminal controls.")?;
    let before =
        std::fs::symlink_metadata(path).map_err(|_| "Cannot inspect the selected file.")?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err("Choose a regular file, not a symlink, directory, pipe, or device.");
    }
    let mut file = open_regular(path)?;
    let metadata = file.metadata().map_err(|_| "Cannot inspect the opened file.")?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_WORKBENCH_IMAGE_BYTES {
        return Err(
            "Image must be a nonempty regular file of at most 4 MiB; nothing was truncated.",
        );
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_WORKBENCH_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the selected file completely.")?;
    if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > MAX_WORKBENCH_IMAGE_BYTES {
        return Err("The file changed size while reading or exceeds 4 MiB; select it again.");
    }
    Ok(ImageBytes { digest: peritus_codec::sha256(&bytes), bytes, label })
}

pub fn validate_explicit_path(path: &Path) -> Result<(), &'static str> {
    if !path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err(
            "Choose an absolute file path without '..'; no path or shell expansion is performed.",
        );
    }
    Ok(())
}

#[cfg(unix)]
pub fn open_regular(path: &Path) -> Result<File, &'static str> {
    use std::os::unix::fs::OpenOptionsExt as _;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| "Cannot open the regular file without following a symlink.")
}
#[cfg(windows)]
pub fn open_regular(path: &Path) -> Result<File, &'static str> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    if path.components().any(|part| matches!(part, Component::Prefix(prefix) if matches!(prefix.kind(), std::path::Prefix::DeviceNS(_)))) {
        return Err("Device namespace imports are not supported.");
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "Cannot open the selected regular file.")?;
    if file.metadata().map_err(|_| "Cannot inspect the opened file.")?.file_attributes()
        & FILE_ATTRIBUTE_REPARSE_POINT
        != 0
    {
        return Err("Symlink and reparse-point imports are not supported.");
    }
    Ok(file)
}
#[cfg(not(any(unix, windows)))]
pub fn open_regular(_path: &Path) -> Result<File, &'static str> {
    Err("Explicit file import is unsupported on this platform.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_read_retains_exact_bytes_and_rejects_relative_escape_empty_and_oversize() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("reference.gif");
        std::fs::write(&path, b"exact bytes; host validates pixels").expect("fixture");
        let image = read(&path).expect("read");
        assert_eq!(image.bytes, b"exact bytes; host validates pixels");
        assert_eq!(image.digest, peritus_codec::sha256(&image.bytes));
        assert!(!format!("{image:?}").contains("host validates"));
        assert!(read(Path::new("reference.gif")).is_err());
        assert!(read(&root.path().join("../reference.gif")).is_err());
        assert!(read(root.path()).is_err());
        let file = File::create(&path).expect("empty");
        assert!(read(&path).is_err());
        file.set_len(MAX_WORKBENCH_IMAGE_BYTES + 1).expect("large sparse fixture");
        assert!(read(&path).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_does_not_import_its_target() {
        let root = tempfile::tempdir().expect("root");
        let target = root.path().join("target");
        std::fs::write(&target, b"private bytes").expect("fixture");
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("link");
        assert!(read(&link).is_err());
    }
}
