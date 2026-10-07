//! Checksummed manifest decoding and protected frame reading.

use std::{
    ffi::OsString,
    io::{ErrorKind, Read},
    os::unix::ffi::OsStringExt,
    path::PathBuf,
};

use peritus_process::CommandSpec;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction, canonical::Reader, error};

use super::{
    CHECKSUM_BYTES, HelperManifest, LEGACY_VERSION, MAGIC, MAX_FRAME_BYTES, NATIVE_VERSION, VERSION,
    fields::{
        decode_containment, decode_environment, decode_environment_legacy, decode_proxy,
        decode_resources, decode_secrets, decode_strings, decode_strings_legacy, decode_terminal,
        expected_preparation, validate_control_environment, validate_executable_path,
        validate_executable_text, validate_protected_handles, validate_working_directory,
    },
};

impl HelperManifest {
    /// Decodes, bounds, checksums, and validates a manifest.
    ///
    /// # Errors
    /// Returns a stable protocol error for malformed, noncanonical, or mismatched bytes.
    #[allow(clippy::too_many_lines, reason = "closed schema decode keeps field order auditable")]
    pub fn decode(input: &[u8]) -> Result<Self, MacosError> {
        if input.len() < MAGIC.len() + 2 + 4 + CHECKSUM_BYTES {
            return Err(error::invalid(MacosOperation::Manifest, "invalid manifest frame size"));
        }
        let checksum_offset = input.len() - CHECKSUM_BYTES;
        let expected_checksum = peritus_codec::sha256(&input[..checksum_offset]);
        let actual_checksum = Sha256Digest::new(
            input[checksum_offset..]
                .try_into()
                .map_err(|_| error::invalid(MacosOperation::Manifest, "invalid checksum size"))?,
        );
        if expected_checksum != actual_checksum {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "helper manifest checksum does not match",
            ));
        }
        let version = u16::from_be_bytes(
            input[MAGIC.len()..MAGIC.len() + 2]
                .try_into()
                .map_err(|_| error::invalid(MacosOperation::Manifest, "invalid version"))?,
        );
        if !matches!(version, LEGACY_VERSION | NATIVE_VERSION | VERSION)
            || (version == LEGACY_VERSION && input.len() > MAX_FRAME_BYTES)
        {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "unknown helper manifest magic or version",
            ));
        }
        let mut envelope = if version == LEGACY_VERSION {
            Reader::new(&input[..checksum_offset])?
        } else {
            Reader::native(&input[..checksum_offset])
        };
        if envelope.fixed::<8>()? != MAGIC || envelope.u16()? != version {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "unknown helper manifest magic or version",
            ));
        }
        let body_length = usize::try_from(envelope.u32()?).map_err(|_| {
            error::limited(MacosOperation::Manifest, "manifest length is too large")
        })?;
        let body = if version == LEGACY_VERSION {
            envelope.bytes()?
        } else {
            envelope.native_bytes()?
        };
        if body.len() != body_length {
            return Err(error::invalid(MacosOperation::Manifest, "manifest length disagrees"));
        }
        envelope.finish()?;
        let mut reader = if version == LEGACY_VERSION {
            Reader::new(body)?
        } else {
            Reader::native(body)
        };
        let process_id = ProcessId::new(reader.fixed()?)
            .map_err(|_| error::invalid(MacosOperation::Manifest, "process identity is zero"))?;
        let plan_digest = Sha256Digest::new(reader.fixed()?);
        let descriptor_digest = Sha256Digest::new(reader.fixed()?);
        let support_digest = Sha256Digest::new(reader.fixed()?);
        let preparation_digest = Sha256Digest::new(reader.fixed()?);
        let profile_digest = Sha256Digest::new(reader.fixed()?);
        let profile = if version == LEGACY_VERSION {
            reader.string()?
        } else {
            String::from_utf8(reader.native_bytes()?.to_vec()).map_err(|_| {
                error::invalid(MacosOperation::Manifest, "Seatbelt profile is not UTF-8")
            })?
        };
        let seatbelt_executable = PathBuf::from(if version == LEGACY_VERSION {
            OsString::from(reader.string()?)
        } else {
            OsString::from_vec(reader.native_bytes()?.to_vec())
        });
        let target_executable = if version == LEGACY_VERSION {
            OsString::from(reader.string()?)
        } else {
            OsString::from_vec(reader.native_bytes()?.to_vec())
        };
        let target_arguments = if version == LEGACY_VERSION {
            decode_strings_legacy(&mut reader)?
        } else {
            decode_strings(&mut reader)?
        };
        let working_directory = PathBuf::from(if version == LEGACY_VERSION {
            OsString::from(reader.string()?)
        } else {
            OsString::from_vec(reader.native_bytes()?.to_vec())
        });
        let environment = if version == LEGACY_VERSION {
            decode_environment_legacy(&mut reader)?
        } else {
            decode_environment(&mut reader)?
        };
        let exec_status_descriptor = reader.u32()?;
        let proxy = decode_proxy(&mut reader)?;
        let resources = decode_resources(&mut reader)?;
        let containment = decode_containment(&mut reader, version)?;
        let terminal = decode_terminal(&mut reader)?;
        let secrets = decode_secrets(&mut reader)?;
        reader.finish()?;
        validate_protected_handles(exec_status_descriptor, proxy.as_ref(), &secrets)?;
        validate_control_environment(&environment, proxy.as_ref(), &secrets)?;
        if expected_preparation(plan_digest, descriptor_digest, support_digest)
            != preparation_digest
        {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "manifest preparation identity does not match its bound facts",
            ));
        }
        if peritus_codec::sha256(profile.as_bytes()) != profile_digest {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "manifest profile digest does not match profile bytes",
            ));
        }
        validate_executable_path(&seatbelt_executable)?;
        validate_executable_text(&target_executable)?;
        CommandSpec::new(target_executable.clone(), target_arguments.clone()).map_err(|_| {
            error::invalid(
                MacosOperation::Manifest,
                "target argv violates the process protocol bounds",
            )
        })?;
        validate_working_directory(&working_directory)?;
        if !resources.is_complete() {
            return Err(MacosError::new(
                MacosErrorKind::UnsupportedHost,
                MacosOperation::Manifest,
                RecoveryAction::SelectSupportedBackend,
                "manifest contains an unsupported resource dimension",
            ));
        }
        let mut manifest = Self {
            encoding_version: version,
            process_id,
            plan_digest,
            descriptor_digest,
            support_digest,
            preparation_digest,
            profile_digest,
            profile,
            seatbelt_executable,
            target_executable,
            target_arguments,
            working_directory,
            environment,
            exec_status_descriptor,
            proxy,
            resources,
            containment,
            terminal,
            secrets,
            canonical: input.to_vec(),
            digest: peritus_codec::sha256(input),
        };
        let reencoded = manifest.encode()?;
        if reencoded != input {
            return Err(error::invalid(MacosOperation::Manifest, "manifest is not canonical"));
        }
        manifest.canonical = reencoded;
        Ok(manifest)
    }

    /// Reads the little-endian length-prefixed manifest frame supplied by C2.
    ///
    /// # Errors
    /// Returns a typed helper error for I/O, truncation, excessive size, or invalid payload.
    pub fn read_framed(mut reader: impl Read) -> Result<Self, MacosError> {
        Self::read_framed_while(&mut reader, || true)
    }

    /// Reads one bounded manifest frame while the helper retains its execution owner.
    ///
    /// A nonblocking production descriptor allows ownership loss to cancel a partial header or
    /// payload without adding an elapsed deadline or retry ceiling.
    ///
    /// # Errors
    /// Returns a typed helper error for cancellation, I/O, truncation, excessive size, or an
    /// invalid payload.
    pub fn read_framed_while(
        mut reader: impl Read,
        mut should_continue: impl FnMut() -> bool,
    ) -> Result<Self, MacosError> {
        let mut length = [0_u8; 4];
        read_exact_while(&mut reader, &mut length, &mut should_continue)?;
        let length = usize::try_from(u32::from_le_bytes(length))
            .map_err(|_| error::limited(MacosOperation::Manifest, "manifest frame is too large"))?;
        if length == 0 || length > MAX_FRAME_BYTES {
            return Err(error::limited(
                MacosOperation::Manifest,
                "manifest frame is empty or exceeds its bound",
            ));
        }
        let mut input = vec![0_u8; length];
        read_exact_while(&mut reader, &mut input, &mut should_continue)?;
        Self::decode(&input)
    }
}

fn read_exact_while(
    reader: &mut impl Read,
    bytes: &mut [u8],
    should_continue: &mut impl FnMut() -> bool,
) -> Result<(), MacosError> {
    let mut offset = 0;
    while offset < bytes.len() {
        if !should_continue() {
            return Err(MacosError::new(
                MacosErrorKind::SupervisorFailure,
                MacosOperation::Manifest,
                RecoveryAction::CancelAndReap,
                "helper manifest delivery was cancelled by its execution owner",
            ));
        }
        match reader.read(&mut bytes[offset..]) {
            Ok(0) => {
                return Err(error::invalid(
                    MacosOperation::Manifest,
                    "helper manifest frame ended before completion",
                ));
            }
            Ok(count) => offset += count,
            Err(source) if source.kind() == ErrorKind::Interrupted => {}
            Err(source) if source.kind() == ErrorKind::WouldBlock => {
                std::thread::yield_now();
            }
            Err(source) => return Err(error::io_error(MacosOperation::Manifest, &source)),
        }
    }
    Ok(())
}
