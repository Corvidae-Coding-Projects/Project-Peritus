//! Native helper protocol fixture implementation used by process integration tests.

use std::io::{Read, Write};

use peritus_process::{native_activation_record, native_ready_record};

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
    let manifest = read_manifest(&mut input)?;
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

fn read_manifest(input: &mut impl Read) -> Result<Vec<u8>, ()> {
    let first = read_length(input)?;
    if first != peritus_process::NATIVE_MANIFEST_STREAM_MARKER {
        return read_page(input, first);
    }
    let mut manifest = Vec::new();
    loop {
        let length = read_length(input)?;
        if length == 0 {
            break;
        }
        let page = read_page(input, length)?;
        manifest.try_reserve(page.len()).map_err(|_| ())?;
        manifest.extend_from_slice(&page);
    }
    if manifest.is_empty() { Err(()) } else { Ok(manifest) }
}

fn read_length(input: &mut impl Read) -> Result<u32, ()> {
    let mut length = [0_u8; 4];
    input.read_exact(&mut length).map_err(|_| ())?;
    Ok(u32::from_le_bytes(length))
}

fn read_page(input: &mut impl Read, length: u32) -> Result<Vec<u8>, ()> {
    let length = usize::try_from(length).map_err(|_| ())?;
    if length == 0 || length > peritus_process::NATIVE_MANIFEST_FRAME_BYTES {
        return Err(());
    }
    let mut page = Vec::new();
    page.try_reserve_exact(length).map_err(|_| ())?;
    page.resize(length, 0);
    input.read_exact(&mut page).map_err(|_| ())?;
    Ok(page)
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
