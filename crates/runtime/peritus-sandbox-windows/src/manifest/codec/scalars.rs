//! Small canonical scalar and socket helpers.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

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
    let count = reader.read_collection_len().map_err(codec_error)?;
    (0..count).map(|_| reader.read_str().map(str::to_owned).map_err(codec_error)).collect()
}

pub(super) fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, WindowsError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec_error)?))
}

pub(super) fn fixed(writer: &mut CanonicalWriter, value: &[u8]) -> Result<(), WindowsError> {
    writer.write_fixed(value).map_err(codec_error)
}

pub(super) fn digest(
    writer: &mut CanonicalWriter,
    value: Sha256Digest,
) -> Result<(), WindowsError> {
    fixed(writer, value.as_bytes())
}

pub(super) fn string(writer: &mut CanonicalWriter, value: &str) -> Result<(), WindowsError> {
    writer.write_str(value).map_err(codec_error)
}

pub(super) fn collection(writer: &mut CanonicalWriter, value: usize) -> Result<(), WindowsError> {
    writer.write_collection_len(value).map_err(codec_error)
}

pub(super) fn u8_value(writer: &mut CanonicalWriter, value: u8) -> Result<(), WindowsError> {
    writer.write_u8(value).map_err(codec_error)
}

pub(super) fn u16_value(writer: &mut CanonicalWriter, value: u16) -> Result<(), WindowsError> {
    writer.write_u16(value).map_err(codec_error)
}

pub(super) fn u32_value(writer: &mut CanonicalWriter, value: u32) -> Result<(), WindowsError> {
    writer.write_u32(value).map_err(codec_error)
}

pub(super) fn u64_value(writer: &mut CanonicalWriter, value: u64) -> Result<(), WindowsError> {
    writer.write_u64(value).map_err(codec_error)
}

pub(super) fn boolean(writer: &mut CanonicalWriter, value: bool) -> Result<(), WindowsError> {
    writer.write_bool(value).map_err(codec_error)
}

pub(super) fn codec_error(_error: peritus_codec::CodecError) -> WindowsError {
    protocol("manifest canonical codec rejected the bounded value")
}

pub(super) fn protocol(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::HelperProtocol,
        WindowsOperation::Manifest,
        WindowsRecovery::RepairHelper,
        detail,
    )
}
