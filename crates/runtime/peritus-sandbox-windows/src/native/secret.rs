//! Exact protected-handle secret staging before target creation.

use std::{
    fs::{File, OpenOptions},
    io::{ErrorKind, Read, Write},
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, RawHandle},
    },
};

use windows_sys::Win32::Storage::FileSystem::{
    FILE_DISPOSITION_INFO, FILE_FLAG_DELETE_ON_CLOSE, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_INFO, FILE_SHARE_READ, FILE_TYPE_DISK,
    FileDispositionInfo, FileIdInfo, GetFileInformationByHandleEx, GetFileType,
    SetFileInformationByHandle,
};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    EnvironmentEntry, HelperManifest, SecretHandleDestination, WindowsError, WindowsErrorKind,
    WindowsOperation, WindowsPath, WindowsRecovery,
};

const READ_CHUNK_BYTES: usize = 64 * 1_024;
const READ_CHUNK_BYTES_U64: u64 = 64 * 1_024;
const MAX_WINDOWS_ENVIRONMENT_UNITS: usize = 32_767;
const MAX_WINDOWS_ENVIRONMENT_UTF8_BYTES: u64 = 3 * 32_767;

pub(super) struct StagedSecrets {
    environment: Vec<EnvironmentEntry>,
    files: Vec<StagedSecretFile>,
}

impl StagedSecrets {
    pub(super) fn environment(&self) -> &[EnvironmentEntry] {
        &self.environment
    }

    pub(super) fn file_identities(
        &self,
    ) -> Result<Vec<peritus_process::NativeWindowsSecretFileIdentity>, WindowsError> {
        let mut identities = Vec::new();
        identities
            .try_reserve_exact(self.files.len())
            .map_err(|_| secret_error("private secret-file identity allocation is unavailable"))?;
        identities.extend(self.files.iter().map(|file| file.identity));
        identities.sort_by(|left, right| {
            left.binding()
                .path_digest()
                .as_bytes()
                .cmp(right.binding().path_digest().as_bytes())
        });
        Ok(identities)
    }

    pub(super) fn commit_files(&mut self) -> Result<(), WindowsError> {
        for file in &mut self.files {
            file.commit()?;
        }
        Ok(())
    }

    pub(super) fn cleanup_files(&mut self) -> Result<(), WindowsError> {
        let mut failed = false;
        for file in &mut self.files {
            if file.cleanup().is_err() {
                failed = true;
            }
        }
        self.files.retain(|file| file.file.is_some());
        if failed {
            Err(secret_error(
                "one or more exact private secret files require reconciliation",
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for StagedSecrets {
    fn drop(&mut self) {
        let _ = self.cleanup_files();
    }
}

struct StagedSecretFile {
    file: Option<File>,
    identity: peritus_process::NativeWindowsSecretFileIdentity,
    committed: bool,
}

impl StagedSecretFile {
    fn commit(&mut self) -> Result<(), WindowsError> {
        if self.committed {
            return Ok(());
        }
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| secret_error("private secret file custody disappeared"))?;
        set_delete_disposition(file, false)?;
        self.committed = true;
        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), WindowsError> {
        let Some(file) = self.file.as_ref() else {
            return Ok(());
        };
        if self.committed {
            set_delete_disposition(file, true)?;
        }
        drop(self.file.take());
        Ok(())
    }
}

impl Drop for StagedSecretFile {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

pub(super) fn stage(
    manifest: &HelperManifest,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<StagedSecrets, WindowsError> {
    validate_environment_declarations(manifest)?;
    let mut staged = StagedSecrets { environment: Vec::new(), files: Vec::new() };
    if let Some(route) = manifest.network().proxy() {
        let mut source = inherited_source(route.routing_handle())?;
        validate_source(&source, 32)?;
        let mut token = read_exact_payload(&mut source, 32, should_continue)?;
        if token.len() != 32 {
            token.zeroize();
            return Err(network_error("managed proxy token has an invalid length"));
        }
        let password = hex(&token);
        token.zeroize();
        let proxy = format!("http://peritus:{password}@{}", route.endpoint());
        let http = EnvironmentEntry::new("HTTP_PROXY", &proxy)?;
        let https = EnvironmentEntry::new("HTTPS_PROXY", proxy)?;
        if manifest.environment().iter().any(|value| {
            crate::manifest::windows_name_cmp(value.name(), http.name()).is_eq()
                || crate::manifest::windows_name_cmp(value.name(), https.name()).is_eq()
        }) {
            return Err(network_error(
                "ordinary environment collides with managed proxy variables",
            ));
        }
        staged.environment.extend([http, https]);
    }
    for descriptor in manifest.secret_handles() {
        if matches!(descriptor.destination(), SecretHandleDestination::Brokered(_)) {
            continue;
        }
        let expected = descriptor.payload_len().ok_or_else(|| {
            secret_error("protected secret lacks an exact payload-length binding")
        })?;
        let mut source = inherited_source(descriptor.handle())?;
        validate_source(&source, expected)?;
        match descriptor.destination() {
            SecretHandleDestination::Environment(name) => {
                if expected > MAX_WINDOWS_ENVIRONMENT_UTF8_BYTES {
                    return Err(secret_error(
                        "environment secret exceeds native Windows capacity",
                    ));
                }
                let mut bytes = read_exact_payload(&mut source, expected, should_continue)?;
                let result = (|| {
                    let text = core::str::from_utf8(&bytes)
                        .map_err(|_| secret_error("environment secret is not valid UTF-8"))?;
                    let units = name
                        .as_str()
                        .encode_utf16()
                        .count()
                        .checked_add(text.encode_utf16().count())
                        .and_then(|value| value.checked_add(3))
                        .ok_or_else(|| {
                            secret_error("environment secret native size overflowed")
                        })?;
                    if units > MAX_WINDOWS_ENVIRONMENT_UNITS {
                        return Err(secret_error(
                            "environment secret exceeds native Windows capacity",
                        ));
                    }
                    staged.environment.push(EnvironmentEntry::new(name.as_str(), text)?);
                    Ok(())
                })();
                bytes.zeroize();
                result?;
            }
            SecretHandleDestination::File(path) => {
                let native = WindowsPath::from_sandbox(manifest.working_directory(), path)?;
                staged.files.push(stage_file(
                    &mut source,
                    expected,
                    native,
                    should_continue,
                )?);
            }
            SecretHandleDestination::Brokered(_) => {}
        }
    }
    staged.environment.sort_by(|left, right| {
        crate::manifest::windows_name_cmp(left.name(), right.name())
    });
    Ok(staged)
}

fn validate_environment_declarations(manifest: &HelperManifest) -> Result<(), WindowsError> {
    use std::os::windows::ffi::OsStrExt as _;

    let mut minimum_units = 1_usize;
    for entry in manifest.environment() {
        minimum_units = minimum_units
            .checked_add(entry.name().encode_wide().count())
            .and_then(|value| value.checked_add(entry.value().encode_wide().count()))
            .and_then(|value| value.checked_add(2))
            .ok_or_else(|| secret_error("native environment declaration overflowed"))?;
    }
    for descriptor in manifest.secret_handles() {
        let SecretHandleDestination::Environment(name) = descriptor.destination() else {
            continue;
        };
        let payload_len = descriptor.payload_len().ok_or_else(|| {
            secret_error("environment secret lacks an exact payload-length binding")
        })?;
        let minimum_value_units = payload_len.div_ceil(3);
        let minimum_value_units = usize::try_from(minimum_value_units)
            .map_err(|_| secret_error("environment secret exceeds native Windows capacity"))?;
        minimum_units = minimum_units
            .checked_add(name.as_str().encode_utf16().count())
            .and_then(|value| value.checked_add(minimum_value_units))
            .and_then(|value| value.checked_add(2))
            .ok_or_else(|| secret_error("native environment declaration overflowed"))?;
    }
    if minimum_units > MAX_WINDOWS_ENVIRONMENT_UNITS {
        return Err(secret_error(
            "declared environment secrets exceed native Windows capacity",
        ));
    }
    Ok(())
}

fn inherited_source(raw: u64) -> Result<File, WindowsError> {
    let raw = usize::try_from(raw)
        .ok()
        .filter(|value| *value != 0 && *value != usize::MAX)
        .ok_or_else(|| secret_error("protected secret handle is invalid"))?;
    // SAFETY: the manifest-bound inherited value is owned by this helper process and each
    // non-brokered descriptor is consumed exactly once into this File owner.
    Ok(unsafe { File::from_raw_handle(raw as RawHandle) })
}

fn validate_source(source: &File, expected: u64) -> Result<(), WindowsError> {
    // SAFETY: the File retains the inherited handle throughout this type query.
    if unsafe { GetFileType(source.as_raw_handle().cast()) } != FILE_TYPE_DISK {
        return Err(secret_error("protected secret source is not a finite regular file"));
    }
    let metadata = source
        .metadata()
        .map_err(|_| secret_error("protected secret source cannot be inspected"))?;
    if !metadata.is_file() || metadata.len() != expected {
        return Err(secret_error(
            "protected secret source length differs from its manifest binding",
        ));
    }
    Ok(())
}

fn read_exact_payload(
    source: &mut File,
    expected: u64,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<Vec<u8>, WindowsError> {
    let expected_usize = usize::try_from(expected)
        .map_err(|_| secret_error("environment secret exceeds native address capacity"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(expected_usize)
        .map_err(|_| secret_error("environment secret allocation is unavailable"))?;
    copy_exact(source, expected, &mut bytes, should_continue)?;
    Ok(bytes)
}

fn stage_file(
    source: &mut File,
    expected: u64,
    path: WindowsPath,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<StagedSecretFile, WindowsError> {
    let parent = path
        .parent()?
        .ok_or_else(|| secret_error("private secret file destination lacks a parent"))?;
    crate::ResolvedWindowsPath::resolve(parent)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_DELETE_ON_CLOSE);
    let mut destination = options
        .open(path.to_path_buf())
        .map_err(|_| secret_error("private secret file cannot be created exclusively"))?;
    copy_exact(source, expected, &mut destination, should_continue)?;
    destination
        .flush()
        .and_then(|()| destination.sync_all())
        .map_err(|_| secret_error("private secret file cannot be synchronized"))?;
    let binding = peritus_process::NativeWindowsSecretFileBinding::new(path.digest(), expected)
        .map_err(|_| secret_error("private secret file binding is invalid"))?;
    let identity = file_identity(&destination, binding)?;
    Ok(StagedSecretFile { file: Some(destination), identity, committed: false })
}

fn copy_exact(
    source: &mut File,
    expected: u64,
    destination: &mut dyn Write,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), WindowsError> {
    let mut buffer = Zeroizing::new([0_u8; READ_CHUNK_BYTES]);
    let mut remaining = expected;
    while remaining != 0 {
        if !should_continue() {
            return Err(secret_error(
                "protected secret read was cancelled by its retained owner",
            ));
        }
        let capacity = usize::try_from(remaining.min(READ_CHUNK_BYTES_U64))
            .map_err(|_| secret_error("protected secret transfer size is not representable"))?;
        let count = match source.read(&mut buffer[..capacity]) {
            Ok(0) => {
                return Err(secret_error(
                    "protected secret ended before its declared payload length",
                ));
            }
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err(secret_error("protected secret source cannot be read")),
        };
        destination
            .write_all(&buffer[..count])
            .map_err(|_| secret_error("protected secret destination cannot be written"))?;
        remaining -= u64::try_from(count)
            .map_err(|_| secret_error("protected secret transfer size is not representable"))?;
    }
    loop {
        if !should_continue() {
            return Err(secret_error(
                "protected secret EOF probe was cancelled by its retained owner",
            ));
        }
        match source.read(&mut buffer[..1]) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                return Err(secret_error(
                    "protected secret exceeds its declared payload length",
                ));
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => return Err(secret_error("protected secret EOF cannot be verified")),
        }
    }
}

fn file_identity(
    file: &File,
    binding: peritus_process::NativeWindowsSecretFileBinding,
) -> Result<peritus_process::NativeWindowsSecretFileIdentity, WindowsError> {
    use std::os::windows::fs::MetadataExt as _;

    if file
        .metadata()
        .map_err(|_| secret_error("private secret file metadata cannot be observed"))?
        .number_of_links()
        != Some(1)
    {
        return Err(secret_error(
            "private secret file does not have exclusive link identity",
        ));
    }
    let mut identity = FILE_ID_INFO::default();
    let size = u32::try_from(core::mem::size_of::<FILE_ID_INFO>())
        .map_err(|_| secret_error("private secret file identity size overflowed"))?;
    // SAFETY: the File is live and `identity` is writable for the exact declared structure size.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle().cast(),
            FileIdInfo,
            (&raw mut identity).cast(),
            size,
        )
    } == 0
    {
        return Err(secret_error("private secret file identity cannot be observed"));
    }
    peritus_process::NativeWindowsSecretFileIdentity::new(
        binding,
        identity.VolumeSerialNumber,
        identity.FileId.Identifier,
    )
    .map_err(|_| secret_error("private secret file identity is incomplete"))
}

fn set_delete_disposition(file: &File, delete: bool) -> Result<(), WindowsError> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: delete };
    let size = u32::try_from(core::mem::size_of::<FILE_DISPOSITION_INFO>())
        .map_err(|_| secret_error("private secret cleanup record size overflowed"))?;
    // SAFETY: the File owns DELETE-capable custody and the disposition record is live and exact.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            FileDispositionInfo,
            (&raw const disposition).cast(),
            size,
        )
    } == 0
    {
        return Err(secret_error(
            "private secret file delete disposition cannot be changed",
        ));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

fn secret_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Secret,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}

fn network_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Network,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}
