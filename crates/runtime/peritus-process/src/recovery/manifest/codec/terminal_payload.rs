//! Complete terminal payload embedded in a durable manifest.

use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use crate::{
    LifecyclePhase, ProcessError, TerminalResult,
    recovery::manifest::ExecutionManifest,
    terminal::{decode_terminal, encode_terminal, terminal_digest},
};

use super::{corrupt, reader::Reader};

pub(super) fn encode_terminal_payload(
    bytes: &mut Vec<u8>,
    terminal: Option<&TerminalResult>,
) -> Result<(), ProcessError> {
    let Some(terminal) = terminal else {
        bytes.push(0);
        return Ok(());
    };
    let encoded = encode_terminal(terminal)?;
    let length = u32::try_from(encoded.len())
        .map_err(|_| corrupt("terminal result length is not representable"))?;
    bytes.push(1);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&encoded);
    Ok(())
}

pub(super) fn decode_terminal_payload(
    reader: &mut Reader<'_>,
    saved_digest: Option<Sha256Digest>,
) -> Result<Option<TerminalResult>, ProcessError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => {
            let length = usize::try_from(reader.u32()?)
                .map_err(|_| corrupt("terminal result length is not representable"))?;
            let bytes = reader.bytes(length)?;
            // Saved results can use an older canonical format. Authenticate those exact bytes
            // before decoding; re-encoding with the current format changes their digest.
            let observed = Sha256Digest::new(Sha256::digest(bytes).into());
            if saved_digest != Some(observed) {
                return Err(corrupt("embedded terminal result checksum differs"));
            }
            decode_terminal(bytes).map(Some)
        }
        _ => Err(corrupt("manifest has an invalid optional terminal-result tag")),
    }
}

pub(super) fn terminal_binding_valid(manifest: &ExecutionManifest) -> Result<bool, ProcessError> {
    match (manifest.phase, manifest.terminal_digest, manifest.terminal.as_ref()) {
        (LifecyclePhase::Terminal, Some(digest), Some(terminal)) => {
            Ok(digest == terminal_digest(terminal)? && manifest.retains_terminal(terminal))
        }
        (LifecyclePhase::Terminal, _, _) => Ok(false),
        (_, None, None) => Ok(true),
        (_, _, _) => Ok(false),
    }
}
