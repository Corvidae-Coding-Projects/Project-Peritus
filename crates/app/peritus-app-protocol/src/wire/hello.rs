//! Canonical client/server negotiation family codecs.

use crate::{
    APP_SCHEMA_V1, AppProtocolLimits, CLIENT_HELLO_FAMILY, ClientHello, ImplementationMetadata,
    IncompatibilityReason, NegotiatedProtocol, NegotiationOutcome, ProtocolId, SERVER_HELLO_FAMILY,
    ServerHello,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use super::primitive::{
    invalid, read_features, read_id, read_limits, read_ranges, read_version, unknown,
    write_features, write_id, write_limits, write_ranges, write_version,
};

impl CanonicalEncode for ClientHello {
    const FAMILY: u16 = CLIENT_HELLO_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_u16(1)?;
        write_id(writer, self.protocol_id().as_bytes())?;
        write_ranges(writer, self.versions())?;
        write_features(writer, self.required_features())?;
        write_features(writer, self.optional_features())?;
        write_limits(writer, self.receive_limits())?;
        writer.write_str(self.implementation().as_str())
    }
}

impl CanonicalDecode for ClientHello {
    const FAMILY: u16 = CLIENT_HELLO_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_client_hello(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_client_hello(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<ClientHello, CodecError> {
    let tag_offset = reader.offset();
    if reader.read_u16()? != 1 {
        return unknown(tag_offset);
    }
    let protocol_id = read_id(reader, ProtocolId::new)?;
    let versions = read_ranges(reader, limits.max_versions())?;
    let required = read_features(reader, limits.max_features())?.into_vec();
    let optional = read_features(reader, limits.max_features())?.into_vec();
    let receive_limits = read_limits(reader)?;
    let implementation = reader.read_str()?.to_owned();
    if implementation.len() > limits.max_diagnostic_bytes() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, reader.offset()));
    }
    invalid(
        reader.offset(),
        ClientHello::new(protocol_id, versions, required, optional, receive_limits, implementation),
    )
}

impl CanonicalEncode for ServerHello {
    const FAMILY: u16 = SERVER_HELLO_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.protocol_id().as_bytes())?;
        writer.write_str(self.implementation().as_str())?;
        match self.outcome() {
            NegotiationOutcome::Compatible(protocol) => {
                writer.write_u16(1)?;
                write_negotiated(writer, protocol)
            }
            NegotiationOutcome::Downgraded(protocol) => {
                writer.write_u16(2)?;
                write_negotiated(writer, protocol)
            }
            NegotiationOutcome::Incompatible(reason) => {
                writer.write_u16(3)?;
                writer.write_u8(reason.tag())?;
                if let IncompatibilityReason::MissingRequiredFeatures(features) = reason {
                    if features.is_empty() {
                        return Err(CodecError::at(
                            CodecErrorKind::InvalidDomainValue,
                            writer.len(),
                        ));
                    }
                    write_features(writer, features)?;
                }
                Ok(())
            }
        }
    }
}

impl CanonicalDecode for ServerHello {
    const FAMILY: u16 = SERVER_HELLO_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_server_hello(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_server_hello(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<ServerHello, CodecError> {
    let protocol_id = read_id(reader, ProtocolId::new)?;
    let implementation_offset = reader.offset();
    let implementation_text = reader.read_str()?.to_owned();
    if implementation_text.len() > limits.max_diagnostic_bytes() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, implementation_offset));
    }
    let implementation =
        invalid(implementation_offset, ImplementationMetadata::new(implementation_text, limits))?;
    let outcome_offset = reader.offset();
    let outcome = match reader.read_u16()? {
        1 => NegotiationOutcome::Compatible(read_negotiated(reader, limits)?),
        2 => NegotiationOutcome::Downgraded(read_negotiated(reader, limits)?),
        3 => {
            let reason_offset = reader.offset();
            let reason = match reader.read_u8()? {
                1 => IncompatibilityReason::NoCommonVersion,
                2 => {
                    let features = read_features(reader, limits.max_features())?;
                    if features.is_empty() {
                        return Err(CodecError::at(
                            CodecErrorKind::InvalidDomainValue,
                            reason_offset,
                        ));
                    }
                    IncompatibilityReason::MissingRequiredFeatures(features)
                }
                _ => return unknown(reason_offset),
            };
            NegotiationOutcome::Incompatible(reason)
        }
        _ => return unknown(outcome_offset),
    };
    Ok(ServerHello::new(protocol_id, implementation, outcome))
}

fn write_negotiated(
    writer: &mut CanonicalWriter,
    value: &NegotiatedProtocol,
) -> Result<(), CodecError> {
    if value.features().len() > value.limits().max_features() {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len()));
    }
    write_version(writer, value.version())?;
    write_features(writer, value.features())?;
    write_limits(writer, value.limits())
}

fn read_negotiated(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<NegotiatedProtocol, CodecError> {
    let version = read_version(reader)?;
    let features = read_features(reader, limits.max_features())?;
    let negotiated_limits = read_limits(reader)?;
    if features.len() > negotiated_limits.max_features() {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset()));
    }
    Ok(NegotiatedProtocol::new(version, features, negotiated_limits))
}
