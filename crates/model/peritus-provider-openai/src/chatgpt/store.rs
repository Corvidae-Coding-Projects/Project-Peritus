//! Owner-protected, bounded atomic OAuth records and cross-process refresh serialization.
use super::{ChatGptAccount, REFERENCE, Session, host_identifier, io_error};
use peritus_provider_core::{CancellationToken, ProviderCoreError};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::Duration,
};
use zeroize::Zeroizing;
const MAX_BYTES: u64 = 256 * 1024;

pub(super) fn reference_path(reference: &str) -> Result<PathBuf, ProviderCoreError> {
    let path = PathBuf::from(reference.strip_prefix(REFERENCE).ok_or_else(io_error)?);
    if !path.is_absolute() || path.extension().and_then(|s| s.to_str()) != Some("json") {
        return Err(io_error());
    }
    Ok(path)
}
pub(super) fn ensure_root(root: &Path) -> Result<(), ProviderCoreError> {
    if !root.is_absolute() {
        return Err(io_error());
    }
    fs::create_dir_all(root).map_err(|_| io_error())?;
    if unsafe_metadata(&fs::symlink_metadata(root).map_err(|_| io_error())?) {
        return Err(io_error());
    }
    check_owner(&fs::symlink_metadata(root).map_err(|_| io_error())?)?;
    protect_directory(root)
}
fn validate_root(root: &Path) -> Result<(), ProviderCoreError> {
    let metadata = fs::symlink_metadata(root).map_err(|_| io_error())?;
    if !metadata.is_dir() || unsafe_metadata(&metadata) {
        return Err(io_error());
    }
    check_owner(&metadata)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io_error());
        }
    }
    Ok(())
}
fn check_owner(metadata: &fs::Metadata) -> Result<(), ProviderCoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.uid() != nix::unistd::geteuid().as_raw() {
            return Err(io_error());
        }
    }
    #[cfg(windows)]
    let _ = metadata;
    Ok(())
}
#[cfg(unix)]
fn protect_directory(root: &Path) -> Result<(), ProviderCoreError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|_| io_error())
}
#[cfg(windows)]
fn protect_directory(root: &Path) -> Result<(), ProviderCoreError> {
    use base64::Engine as _;
    let script = "$ErrorActionPreference='Stop'; $p=$env:PERITUS_OAUTH_DIRECTORY; $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User; $acl=[Security.AccessControl.DirectorySecurity]::new(); $acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false); foreach($identity in @($sid,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'),[Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))){$rule=[Security.AccessControl.FileSystemAccessRule]::new($identity,'FullControl','ContainerInherit,ObjectInherit','None','Allow');$acl.AddAccessRule($rule)};Set-Acl -LiteralPath $p -AclObject $acl";
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>());
    use std::os::windows::process::CommandExt as _;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(io_error)?;
    let executable = system_root.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child = std::process::Command::new(executable)
        .creation_flags(0x0800_0000)
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .env("PERITUS_OAUTH_DIRECTORY", root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| io_error())?;
    let started = std::time::Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if started.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => break false,
        }
    };
    if !success {
        let _ = child.kill();
    }
    let _ = child.wait();
    if success { Ok(()) } else { Err(io_error()) }
}
#[cfg(unix)]
fn unsafe_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
#[cfg(windows)]
fn unsafe_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}
fn open_private(path: &Path, create: bool) -> Result<File, ProviderCoreError> {
    validate_root(path.parent().ok_or_else(io_error)?)?;
    if path.exists() && unsafe_metadata(&fs::symlink_metadata(path).map_err(|_| io_error())?) {
        return Err(io_error());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(0x00200000);
    }
    let file = options.open(path).map_err(|_| io_error())?;
    let metadata = file.metadata().map_err(|_| io_error())?;
    if !metadata.is_file() || unsafe_metadata(&metadata) {
        return Err(io_error());
    }
    check_owner(&metadata)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io_error());
        }
    }
    Ok(file)
}
pub(super) fn load(path: &Path) -> Result<Session, ProviderCoreError> {
    let file = open_private(path, false)?;
    if file.metadata().map_err(|_| io_error())?.len() > MAX_BYTES {
        return Err(io_error());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| io_error())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io_error());
    }
    let session: Session = serde_json::from_slice(&bytes).map_err(|_| io_error())?;
    session.validate()?;
    Ok(session)
}
pub(super) fn save(path: &Path, session: &Session) -> Result<(), ProviderCoreError> {
    session.validate()?;
    let root = path.parent().ok_or_else(io_error)?;
    ensure_root(root)?;
    if path.exists() && unsafe_metadata(&fs::symlink_metadata(path).map_err(|_| io_error())?) {
        return Err(io_error());
    }
    let bytes = Zeroizing::new(serde_json::to_vec(session).map_err(|_| io_error())?);
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io_error());
    }
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(|_| io_error())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| io_error())?;
    }
    file.write_all(&bytes).and_then(|()| file.as_file().sync_all()).map_err(|_| io_error())?;
    file.persist(path).map_err(|_| io_error())?;
    #[cfg(unix)]
    File::open(root).and_then(|file| file.sync_all()).map_err(|_| io_error())?;
    Ok(())
}
pub(super) fn list(root: &Path) -> Result<Vec<ChatGptAccount>, ProviderCoreError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    validate_root(root)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(root).map_err(|_| io_error())? {
        let path = entry.map_err(|_| io_error())?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            entries.push(ChatGptAccount::from_session(path.clone(), &load(&path)?));
        }
    }
    entries.sort_by(|a, b| a.label().cmp(b.label()));
    Ok(entries)
}
pub(super) fn host_id(root: &Path) -> Result<String, ProviderCoreError> {
    ensure_root(root)?;
    let path = root.join("host-id");
    if path.exists() {
        let mut value = String::new();
        open_private(&path, false)?.take(257).read_to_string(&mut value).map_err(|_| io_error())?;
        if !super::valid_host(&value) {
            return Err(io_error());
        }
        return Ok(value);
    }
    let value = host_identifier()?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            file.write_all(value.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|_| io_error())?;
            Ok(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => host_id(root),
        Err(_) => Err(io_error()),
    }
}
pub(super) async fn lock(
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<File, ProviderCoreError> {
    let lock = path.with_extension("lock");
    let file = open_private(&lock, true)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        if cancellation.is_cancelled() {
            return Err(ProviderCoreError::cancelled("chatgpt_refresh"));
        }
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) => {}
            Err(_) => return Err(io_error()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(super::auth(
                "another ChatGPT session operation is still running; retry shortly",
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
