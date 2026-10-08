//! Durable ownership and retained stop controls for one supervised configuration.

use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Write as _},
    path::{Component, Path, PathBuf},
};

pub(super) struct ControlPaths {
    pub(super) lock: PathBuf,
    pub(super) owner: PathBuf,
    pub(super) stop: PathBuf,
}

pub(super) struct SupervisorLease {
    lock: File,
    owner: PathBuf,
    pub(super) token: String,
}

impl Drop for SupervisorLease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.owner);
        let _ = fs4::FileExt::unlock(&self.lock);
    }
}

pub(super) fn control_paths(configuration: &OsStr) -> io::Result<ControlPaths> {
    let configuration = Path::new(configuration);
    if !configuration.is_absolute()
        || configuration
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "supervised daemon configuration must be an absolute normalized path",
        ));
    }
    Ok(ControlPaths {
        lock: append_suffix(configuration, ".supervisor.lock"),
        owner: append_suffix(configuration, ".supervisor.owner"),
        stop: append_suffix(configuration, ".supervisor.stop"),
    })
}

pub(super) fn acquire_lease(control: &ControlPaths) -> io::Result<SupervisorLease> {
    let lock = open_lock(&control.lock)?;
    fs4::FileExt::try_lock(&lock).map_err(|error| match error {
        fs4::TryLockError::WouldBlock => io::Error::new(
            io::ErrorKind::AlreadyExists,
            "another daemon supervisor owns this configuration",
        ),
        fs4::TryLockError::Error(error) => error,
    })?;
    remove_if_present(&control.owner)?;
    remove_if_present(&control.stop)?;
    let token = owner_token()?;
    let mut owner = OpenOptions::new().write(true).create_new(true).open(&control.owner)?;
    owner.write_all(token.as_bytes())?;
    owner.sync_all()?;
    Ok(SupervisorLease { lock, owner: control.owner.clone(), token })
}

pub(super) fn open_lock(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)
}

pub(super) fn publish_stop(path: &Path, token: &str) -> io::Result<()> {
    let mut marker = OpenOptions::new().write(true).create(true).truncate(true).open(path)?;
    marker.write_all(token.as_bytes())?;
    marker.sync_all()
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn owner_token() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| io::Error::other("supervisor owner randomness is unavailable"))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut token, "{byte:02x}")
            .map_err(|_| io::Error::other("supervisor owner token cannot be encoded"))?;
    }
    Ok(token)
}
