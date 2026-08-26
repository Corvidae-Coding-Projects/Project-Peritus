//! Canonical stable application error representation.

use crate::{
    AppDiagnostic, AppErrorCode, AppProtocolError, AppProtocolLimits, ResponsibleSubsystem,
    RetryDisposition,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

use super::primitive::{invalid, read_string_option, unknown, write_string_option};

pub(super) fn write_app_error(
    writer: &mut CanonicalWriter,
    value: &AppProtocolError,
) -> Result<(), CodecError> {
    writer.write_u16(value.code().tag())?;
    writer.write_u8(value.retry().tag())?;
    writer.write_u8(value.subsystem().tag())?;
    write_string_option(writer, value.diagnostic().map(AppDiagnostic::as_str))
}

pub(super) fn read_app_error(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<AppProtocolError, CodecError> {
    let code_offset = reader.offset();
    let code =
        AppErrorCode::from_tag(reader.read_u16()?).map_or_else(|| unknown(code_offset), Ok)?;
    let retry_offset = reader.offset();
    let retry =
        RetryDisposition::from_tag(reader.read_u8()?).map_or_else(|| unknown(retry_offset), Ok)?;
    let subsystem_offset = reader.offset();
    let subsystem = ResponsibleSubsystem::from_tag(reader.read_u8()?)
        .map_or_else(|| unknown(subsystem_offset), Ok)?;
    let diagnostic = match read_string_option(reader)? {
        Some(value) => {
            let offset = reader.offset();
            if value.len() > limits.max_diagnostic_bytes() {
                return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
            }
            Some(invalid(offset, AppDiagnostic::new(value, limits.max_diagnostic_bytes()))?)
        }
        None => None,
    };
    Ok(AppProtocolError::classified(code, retry, subsystem, diagnostic))
}
