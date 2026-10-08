//! Filesystem links and atomic package-pointer publication.

#[cfg(windows)]
struct OwnedHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
#[allow(unsafe_code, reason = "the owned Win32 package handle is closed exactly once")]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the constructor stores only a non-invalid handle returned by CreateFileW.
        let _ = unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(unix)]
pub(super) fn is_filesystem_link(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
pub(super) fn remove_filesystem_link(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::remove_file(path)
}

#[cfg(windows)]
pub(super) fn is_filesystem_link(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;

    metadata.file_attributes()
        & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
        != 0
}

#[cfg(windows)]
pub(super) fn remove_filesystem_link(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::remove_dir(path)
}

#[cfg(unix)]
pub(super) fn atomic_replace(candidate: &std::path::Path, current: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(candidate, current)
}

#[cfg(unix)]
pub(super) fn sync_parent(parent: &std::path::Path) -> std::io::Result<()> {
    std::fs::File::open(parent)?.sync_all()
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "atomic junction replacement requires the documented FileRenameInfoEx boundary"
)]
pub(super) fn atomic_replace(candidate: &std::path::Path, current: &std::path::Path) -> std::io::Result<()> {
    use std::{
        mem::{offset_of, size_of},
        os::windows::ffi::OsStrExt as _,
        ptr,
    };

    use windows_sys::Win32::{
        Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FileRenameInfoEx, OPEN_EXISTING, SetFileInformationByHandle,
        },
        System::WindowsProgramming::{
            FILE_RENAME_FLAG_POSIX_SEMANTICS, FILE_RENAME_FLAG_REPLACE_IF_EXISTS,
        },
    };

    let candidate_wide = candidate
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: the NUL-terminated path remains live for the synchronous CreateFileW call.
    let handle = unsafe {
        CreateFileW(
            candidate_wide.as_ptr(),
            DELETE | GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let handle = OwnedHandle(handle);
    let target = current.as_os_str().encode_wide().collect::<Vec<_>>();
    let name_bytes = target
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is too long")
        })?;
    let byte_length = offset_of!(FILE_RENAME_INFO, FileName)
        .checked_add(usize::try_from(name_bytes).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is too long")
        })?)
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is too long")
        })?;
    let mut storage = vec![0_usize; byte_length.div_ceil(size_of::<usize>())];
    let information = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: storage is aligned for FILE_RENAME_INFO and sized for its header plus target name.
    unsafe {
        (*information).Anonymous.Flags =
            FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS;
        (*information).RootDirectory = ptr::null_mut();
        (*information).FileNameLength = name_bytes;
        ptr::copy_nonoverlapping(
            target.as_ptr(),
            ptr::addr_of_mut!((*information).FileName).cast::<u16>(),
            target.len(),
        );
    }
    let information_size = u32::try_from(byte_length).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is too long")
    })?;
    // SAFETY: handle owns the candidate junction and information names one initialized buffer.
    if unsafe {
        SetFileInformationByHandle(
            handle.0,
            FileRenameInfoEx,
            information.cast(),
            information_size,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
pub(super) const fn sync_parent(_parent: &std::path::Path) -> std::io::Result<()> {
    // FileRenameInfoEx is the atomic metadata operation; retained receipts are separately flushed.
    Ok(())
}
