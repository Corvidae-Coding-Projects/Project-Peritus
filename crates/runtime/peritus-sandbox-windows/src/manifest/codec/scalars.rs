//! Small canonical scalar and socket helpers.

use std::{
    ffi::{OsStr, OsString},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimit};
use peritus_types::Sha256Digest;

use crate::{
    WindowsError, WindowsErrorKind, WindowsErrorSource, WindowsOperation, WindowsRecovery,
};

pub(super) fn encode_socket(
    writer: &mut CanonicalWriter,
    value: SocketAddr,
) -> Result<(), WindowsError> {
    match value.ip() {
        IpAddr::V4(address) => {
            u8_value(writer, 4)?;
            fixed(writer, &address.octets())?;
        }
        IpAddr::V6(address) => {
            u8_value(writer, 6)?;
            fixed(writer, &address.octets())?;
        }
    }
    u16_value(writer, value.port())
}

pub(super) fn decode_socket(reader: &mut CanonicalReader<'_>) -> Result<SocketAddr, WindowsError> {
    let ip = match reader.read_u8().map_err(codec_error)? {
        4 => IpAddr::V4(Ipv4Addr::from(reader.read_fixed::<4>().map_err(codec_error)?)),
        6 => IpAddr::V6(Ipv6Addr::from(reader.read_fixed::<16>().map_err(codec_error)?)),
        _ => return Err(protocol("manifest proxy address family is unknown")),
    };
    Ok(SocketAddr::new(ip, reader.read_u16().map_err(codec_error)?))
}

pub(super) fn strings(writer: &mut CanonicalWriter, values: &[String]) -> Result<(), WindowsError> {
    collection(writer, values.len())?;
    for value in values {
        string(writer, value)?;
    }
    Ok(())
}

pub(super) fn read_strings(reader: &mut CanonicalReader<'_>) -> Result<Vec<String>, WindowsError> {
    let count = reader.read_collection_len(4).map_err(codec_error)?;
    let mut strings = reader.reserve_collection(count).map_err(codec_error)?;
    for _ in 0..count {
        strings.push(reader.read_str().map_err(codec_error)?.to_owned());
    }
    Ok(strings)
}

pub(super) fn os_string(
    writer: &mut CanonicalWriter,
    value: &OsStr,
) -> Result<(), WindowsError> {
    let units = wide_units(value);
    let byte_count = units.len().checked_mul(2).ok_or_else(|| {
        WindowsError::new(
            WindowsErrorKind::InvalidPlan,
            WindowsOperation::Manifest,
            WindowsRecovery::CorrectRequest,
            "manifest native-text byte length is not representable",
        )
    })?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(byte_count).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidPlan,
            WindowsOperation::Manifest,
            WindowsRecovery::CorrectRequest,
            "manifest native-text encoding allocation is unavailable",
        )
    })?;
    for unit in units {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    writer.write_bytes(&bytes).map_err(encode_error)
}

pub(super) fn os_strings(
    writer: &mut CanonicalWriter,
    values: &[OsString],
) -> Result<(), WindowsError> {
    collection(writer, values.len())?;
    for value in values {
        os_string(writer, value)?;
    }
    Ok(())
}

pub(super) fn read_os_string(
    reader: &mut CanonicalReader<'_>,
) -> Result<OsString, WindowsError> {
    let bytes = reader.read_bytes().map_err(codec_error)?;
    let chunks = bytes.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(protocol("native Windows text has an odd byte length"));
    }
    let mut units = Vec::new();
    units.try_reserve_exact(bytes.len() / 2).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::HelperProtocol,
            WindowsOperation::Manifest,
            WindowsRecovery::RepairHelper,
            "manifest native-text decoding allocation is unavailable",
        )
    })?;
    units.extend(chunks.map(|pair| u16::from_be_bytes([pair[0], pair[1]])));
    os_from_units(&units)
}

pub(super) fn read_os_strings(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<OsString>, WindowsError> {
    let count = reader.read_collection_len(4).map_err(codec_error)?;
    let mut values = reader.reserve_collection(count).map_err(codec_error)?;
    for _ in 0..count {
        values.push(read_os_string(reader)?);
    }
    Ok(values)
}

#[cfg(windows)]
fn wide_units(value: &OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide().collect()
}

#[cfg(not(windows))]
fn wide_units(value: &OsStr) -> Vec<u16> {
    value.to_string_lossy().encode_utf16().collect()
}

#[cfg(windows)]
fn os_from_units(units: &[u16]) -> Result<OsString, WindowsError> {
    use std::os::windows::ffi::OsStringExt;

    Ok(OsString::from_wide(units))
}

#[cfg(not(windows))]
fn os_from_units(units: &[u16]) -> Result<OsString, WindowsError> {
    String::from_utf16(units)
        .map(Into::into)
        .map_err(|_| protocol("native Windows text cannot be represented on this host"))
}

pub(super) fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, WindowsError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec_error)?))
}

pub(super) fn fixed(writer: &mut CanonicalWriter, value: &[u8]) -> Result<(), WindowsError> {
    writer.write_fixed(value).map_err(encode_error)
}

pub(super) fn digest(
    writer: &mut CanonicalWriter,
    value: Sha256Digest,
) -> Result<(), WindowsError> {
    fixed(writer, value.as_bytes())
}

pub(super) fn string(writer: &mut CanonicalWriter, value: &str) -> Result<(), WindowsError> {
    writer.write_str(value).map_err(encode_error)
}

pub(super) fn collection(writer: &mut CanonicalWriter, value: usize) -> Result<(), WindowsError> {
    writer.write_collection_len(value).map_err(encode_error)
}

pub(super) fn u8_value(writer: &mut CanonicalWriter, value: u8) -> Result<(), WindowsError> {
    writer.write_u8(value).map_err(encode_error)
}

pub(super) fn u16_value(writer: &mut CanonicalWriter, value: u16) -> Result<(), WindowsError> {
    writer.write_u16(value).map_err(encode_error)
}

pub(super) fn u32_value(writer: &mut CanonicalWriter, value: u32) -> Result<(), WindowsError> {
    writer.write_u32(value).map_err(encode_error)
}

pub(super) fn u64_value(writer: &mut CanonicalWriter, value: u64) -> Result<(), WindowsError> {
    writer.write_u64(value).map_err(encode_error)
}

pub(super) fn boolean(writer: &mut CanonicalWriter, value: bool) -> Result<(), WindowsError> {
    writer.write_bool(value).map_err(encode_error)
}

pub(super) fn encode_error(error: CodecError) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::InvalidPlan,
        WindowsOperation::Manifest,
        WindowsRecovery::CorrectRequest,
        codec_detail(error, "manifest encoding failed"),
    )
    .with_source(WindowsErrorSource::Codec { kind: error.kind(), limit: error.limit() })
}

pub(super) fn codec_error(error: CodecError) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::HelperProtocol,
        WindowsOperation::Manifest,
        WindowsRecovery::RepairHelper,
        codec_detail(error, "manifest canonical value is invalid"),
    )
    .with_source(WindowsErrorSource::Codec { kind: error.kind(), limit: error.limit() })
}

const fn codec_detail(error: CodecError, fallback: &'static str) -> &'static str {
    match error.limit() {
        Some(CodecLimit::FrameBytes) => "manifest complete frame exceeds physical capacity",
        Some(CodecLimit::PayloadBytes) => "manifest frame payload exceeds physical capacity",
        Some(CodecLimit::CollectionItems) => {
            "manifest collection count exceeds its selected representation"
        }
        Some(CodecLimit::StringBytes) => {
            "manifest UTF-8 field exceeds its selected representation"
        }
        Some(CodecLimit::OpaqueBytes) => {
            "manifest native-text field exceeds its selected representation"
        }
        Some(CodecLimit::NestingDepth) => "manifest nesting exceeds its selected representation",
        None => match error.kind() {
            CodecErrorKind::LengthOverflow => "manifest length exceeds its wire representation",
            CodecErrorKind::AllocationUnavailable => "manifest allocation is unavailable",
            _ => fallback,
        },
    }
}

pub(super) fn protocol(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::HelperProtocol,
        WindowsOperation::Manifest,
        WindowsRecovery::RepairHelper,
        detail,
    )
}
