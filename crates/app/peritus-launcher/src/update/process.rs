//! Exact-birth native child custody for update helpers.

use std::{
    fs::{File, OpenOptions},
    io::Read as _,
    process::{Child, Command, ExitStatus, Stdio},
};

use peritus_process::{
    NativeProcessProbe, ProcessProbe as _, ProcessTreeIdentity, ProcessTreeQuiescence,
};

use crate::LauncherError;

const MAX_CAPTURED_STDOUT_BYTES: u64 = 64 * 1024;

pub(super) fn status(
    command: &mut Command,
    operation: &'static str,
    spawned: impl FnOnce(ProcessTreeIdentity) -> Result<(), LauncherError>,
) -> Result<ExitStatus, LauncherError> {
    configure_group(command);
    let mut child = command
        .spawn()
        .map_err(|error| LauncherError::Update(format!("{operation}: {error}")))?;
    let root_pid = child.id();
    let mut probe = NativeProcessProbe::new();
    let identity = match probe.capture_isolated_child(root_pid) {
        Ok(identity) => identity,
        Err(error) => {
            stop_unidentified(&mut child);
            return Err(LauncherError::Update(format!(
                "{operation}: capture native child ownership: {error}"
            )));
        }
    };
    if let Err(error) = spawned(identity) {
        stop_owned(&mut child, identity, &mut probe);
        return Err(error);
    }
    let result = child
        .wait()
        .map_err(|error| LauncherError::Update(format!("{operation}: wait failed: {error}")))?;
    #[cfg(unix)]
    if probe
        .observe_quiescence(identity)
        .map_err(|error| LauncherError::Update(format!(
            "{operation}: observe native helper quiescence: {error}"
        )))?
        != ProcessTreeQuiescence::Quiescent
    {
        return Err(LauncherError::Update(format!(
            "{operation}: the exact helper process group retained descendants"
        )));
    }
    Ok(result)
}

pub(super) fn stdout(
    command: &mut Command,
    operation: &'static str,
    capture: &std::path::Path,
) -> Result<(ExitStatus, Vec<u8>), LauncherError> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(capture)
        .map_err(|error| LauncherError::filesystem("create update helper output", capture, error))?;
    crate::persistence::protect_file(&file, capture)?;
    let child_output = file.try_clone().map_err(|error| {
        LauncherError::filesystem("duplicate update helper output", capture, error)
    })?;
    command.stdout(Stdio::from(child_output));
    let result = status(command, operation, |_| Ok(()));
    file.sync_all().map_err(|error| {
        LauncherError::filesystem("synchronize update helper output", capture, error)
    })?;
    let status = result?;
    let metadata = file.metadata().map_err(|error| {
        LauncherError::filesystem("inspect update helper output", capture, error)
    })?;
    if metadata.len() > MAX_CAPTURED_STDOUT_BYTES {
        return Err(LauncherError::Update(format!(
            "{operation}: stdout exceeded {MAX_CAPTURED_STDOUT_BYTES} bytes"
        )));
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| {
        LauncherError::Update(format!("{operation}: stdout length is not representable"))
    })?;
    let mut bytes = Vec::new();
    bytes.try_reserve(capacity).map_err(|_| {
        LauncherError::Update(format!("{operation}: stdout allocation is unavailable"))
    })?;
    File::open(capture)
        .and_then(|mut input| input.read_to_end(&mut bytes))
        .map_err(|error| LauncherError::filesystem("read update helper output", capture, error))?;
    Ok((status, bytes))
}

fn stop_owned(
    child: &mut Child,
    identity: ProcessTreeIdentity,
    probe: &mut NativeProcessProbe,
) {
    if identity.complete_containment() {
        let _ = probe.terminate(identity);
    } else {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn stop_unidentified(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn configure_group(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

#[cfg(windows)]
fn configure_group(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn configure_group(command: &mut Command) {
    let _ = command.get_program();
}
