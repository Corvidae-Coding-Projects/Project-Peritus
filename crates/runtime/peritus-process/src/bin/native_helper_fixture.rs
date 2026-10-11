//! Native helper protocol fixture implementation used by process integration tests.

use std::io::{Read, Write};

use peritus_process::{native_activation_record, native_ready_record};

const MAX_MANIFEST_BYTES: usize = 4 * 1_024 * 1_024;
const PROTECTED_MARKER: &[u8] = b"peritus-native-protected-test-v1\0";
#[cfg(unix)]
const PROTECTED_PAYLOAD: &[u8] = b"peritus-protected-test-payload";

/// Runs one bounded native-helper fixture exchange and then the literal target.
///
/// # Errors
/// Returns an opaque fixture failure before the literal target is executed.
#[allow(clippy::result_unit_err, reason = "fixture failures map to one reserved helper exit code")]
pub fn run() -> Result<(), ()> {
    #[cfg(unix)]
    let pty = peritus_process::NativePtyAttachment::from_environment().map_err(|_| ())?;
    let mut output = std::io::stdout().lock();
    output.write_all(native_ready_record().as_bytes()).map_err(|_| ())?;
    output.flush().map_err(|_| ())?;

    let mut input = std::io::stdin().lock();
    let mut length = [0_u8; 4];
    input.read_exact(&mut length).map_err(|_| ())?;
    let length = usize::try_from(u32::from_le_bytes(length)).map_err(|_| ())?;
    if length == 0 || length > MAX_MANIFEST_BYTES {
        return Err(());
    }
    let mut manifest = vec![0_u8; length];
    input.read_exact(&mut manifest).map_err(|_| ())?;
    if manifest.len() < 32 {
        return Err(());
    }
    let preparation = peritus_types::Sha256Digest::new(manifest[..32].try_into().map_err(|_| ())?);
    verify_protected_payload(&manifest[32..])?;
    let manifest_digest = peritus_codec::sha256(&manifest);
    output
        .write_all(native_activation_record(manifest_digest, preparation).as_bytes())
        .map_err(|_| ())?;
    output.flush().map_err(|_| ())?;
    drop(output);
    drop(input);

    let mut arguments = std::env::args_os().skip(1);
    let executable = arguments.next().ok_or(())?;
    let mut command = std::process::Command::new(executable);
    command.args(arguments);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        if let Some(pty) = pty {
            pty.configure(&mut command).map_err(|_| ())?;
        }
        let _error = command.exec();
        Err(())
    }
    #[cfg(windows)]
    {
        let status = command.status().map_err(|_| ())?;
        std::process::exit(status.code().unwrap_or(125));
    }
}

#[cfg(unix)]
fn verify_protected_payload(body: &[u8]) -> Result<(), ()> {
    if !body.starts_with(PROTECTED_MARKER) {
        return Ok(());
    }
    let raw = body.get(PROTECTED_MARKER.len()..PROTECTED_MARKER.len() + 8).ok_or(())?;
    let descriptor = u64::from_le_bytes(raw.try_into().map_err(|_| ())?);
    let mut file = std::fs::File::open(format!("/dev/fd/{descriptor}")).map_err(|_| ())?;
    let mut payload = Vec::new();
    file.read_to_end(&mut payload).map_err(|_| ())?;
    if payload == PROTECTED_PAYLOAD { Ok(()) } else { Err(()) }
}

#[cfg(windows)]
fn verify_protected_payload(body: &[u8]) -> Result<(), ()> {
    if body.starts_with(PROTECTED_MARKER) { Err(()) } else { Ok(()) }
}
