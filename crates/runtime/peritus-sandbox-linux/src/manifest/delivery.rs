//! Canonical secret-destination encoding without secret payload material.

use super::manifest_error;
use crate::LinuxError;
use peritus_sandbox::{BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery};

pub(super) fn encode_legacy(
    bytes: &mut Vec<u8>,
    delivery: &SecretDelivery,
) -> Result<(), LinuxError> {
    let (tag, value) = match delivery {
        SecretDelivery::Environment(name) => (0, name.as_str()),
        SecretDelivery::File(path) => (1, path.as_str()),
        SecretDelivery::BrokeredHandle(label) => (2, label.as_str()),
    };
    bytes.push(tag);
    crate::canonical::push_str(bytes, value)
}

pub(super) fn encode_v2(
    bytes: &mut Vec<u8>,
    delivery: &SecretDelivery,
) -> Result<(), LinuxError> {
    let value = match delivery {
        SecretDelivery::Environment(name) => {
            bytes.push(0);
            name.as_str()
        }
        SecretDelivery::File(path) => {
            bytes.push(1);
            path.as_str()
        }
        SecretDelivery::BrokeredHandle(label) => {
            bytes.push(2);
            label.as_str()
        }
    };
    crate::canonical::push_bytes_unbounded(bytes, value.as_bytes())
}

pub(super) fn decode(
    reader: &mut crate::canonical::Reader<'_>,
) -> Result<SecretDelivery, LinuxError> {
    match reader.u8()? {
        0 => EnvironmentName::new(reader.string()?)
            .map(SecretDelivery::Environment)
            .map_err(|_| manifest_error("protected environment destination is invalid")),
        1 => SandboxPath::new(reader.string()?)
            .map(SecretDelivery::File)
            .map_err(|_| manifest_error("protected file destination is invalid")),
        2 => BrokeredHandleLabel::new(reader.string()?)
            .map(SecretDelivery::BrokeredHandle)
            .map_err(|_| manifest_error("protected brokered destination is invalid")),
        _ => Err(manifest_error("protected payload destination tag is invalid")),
    }
}

pub(super) fn decode_v2(
    reader: &mut crate::canonical::Reader<'_>,
) -> Result<SecretDelivery, LinuxError> {
    match reader.u8()? {
        0 => EnvironmentName::new(reader.string_unbounded()?)
            .map(SecretDelivery::Environment)
            .map_err(|_| manifest_error("protected environment destination is invalid")),
        1 => SandboxPath::new(reader.string_unbounded()?)
            .map(SecretDelivery::File)
            .map_err(|_| manifest_error("protected file destination is invalid")),
        2 => BrokeredHandleLabel::new(reader.string_unbounded()?)
            .map(SecretDelivery::BrokeredHandle)
            .map_err(|_| manifest_error("protected brokered destination is invalid")),
        _ => Err(manifest_error("protected payload destination tag is invalid")),
    }
}
