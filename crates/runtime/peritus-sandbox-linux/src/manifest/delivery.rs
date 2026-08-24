//! Canonical secret-destination encoding without secret payload material.

use super::manifest_error;
use crate::LinuxError;
use peritus_sandbox::{BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery};

pub(super) fn encode(bytes: &mut Vec<u8>, delivery: &SecretDelivery) -> Result<(), LinuxError> {
    match delivery {
        SecretDelivery::Environment(name) => {
            bytes.push(0);
            crate::canonical::push_str(bytes, name.as_str())?;
        }
        SecretDelivery::File(path) => {
            bytes.push(1);
            crate::canonical::push_str(bytes, path.as_str())?;
        }
        SecretDelivery::BrokeredHandle(label) => {
            bytes.push(2);
            crate::canonical::push_str(bytes, label.as_str())?;
        }
    }
    Ok(())
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
