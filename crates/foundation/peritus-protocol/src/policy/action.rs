//! Complete canonical bytes for one proposed action intent.

#![allow(
    clippy::missing_errors_doc,
    reason = "action codecs and digest helpers use the shared CodecError vocabulary"
)]

use super::tags::{operation_class_tag, read_operation_class};
use crate::SCHEMA_V1;
use crate::primitive::{read_id, read_role, write_id, write_role};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits, canonical_sha256,
};
use peritus_kernel::KernelCommand;
use peritus_policy::{ActorRole, OperationClass};
use peritus_types::{
    ActionId, ActorId, CapabilityName, EnvironmentId, ResourceId, Sha256Digest, TurnId,
};

/// Exact action content whose complete canonical frame is bound by B0 and B1 digests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionIntentDto {
    /// Stable action identity.
    pub action_id: ActionId,
    /// Actor requesting the operation.
    pub actor_id: ActorId,
    /// Compiled role used by role separation.
    pub role: ActorRole,
    /// Isolated execution environment.
    pub environment_id: EnvironmentId,
    /// Exact target resource.
    pub resource_id: ResourceId,
    /// Exact registered capability name.
    pub capability_name: CapabilityName,
    /// Compiled operation category.
    pub operation_class: OperationClass,
    /// Payload media type interpreted by the named adapter.
    pub media_type: String,
    /// Opaque adapter request bytes.
    pub payload: Vec<u8>,
}

impl ActionIntentDto {
    /// Hashes the complete canonical frame, including family and schema version.
    pub fn digest(&self, limits: CodecLimits) -> Result<Sha256Digest, CodecError> {
        canonical_sha256(self, limits)
    }

    /// Produces the B0 action proposal bound to these exact canonical bytes.
    pub fn propose_command(
        &self,
        turn_id: TurnId,
        limits: CodecLimits,
    ) -> Result<KernelCommand, CodecError> {
        Ok(KernelCommand::ProposeAction {
            turn_id,
            action_id: self.action_id,
            digest: self.digest(limits)?,
            actor_id: self.actor_id,
            role: self.role,
            environment_id: self.environment_id,
        })
    }
}

impl CanonicalEncode for ActionIntentDto {
    const FAMILY: u16 = 20;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.action_id.as_bytes())?;
        write_id(writer, self.actor_id.as_bytes())?;
        write_role(writer, self.role)?;
        write_id(writer, self.environment_id.as_bytes())?;
        write_id(writer, self.resource_id.as_bytes())?;
        writer.write_str(self.capability_name.as_str())?;
        writer.write_u16(operation_class_tag(self.operation_class))?;
        writer.write_str(&self.media_type)?;
        writer.write_bytes(&self.payload)
    }
}

impl CanonicalDecode for ActionIntentDto {
    const FAMILY: u16 = 20;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let action_id = read_id(reader, ActionId::new)?;
        let actor_id = read_id(reader, ActorId::new)?;
        let role = read_role(reader)?;
        let environment_id = read_id(reader, EnvironmentId::new)?;
        let resource_id = read_id(reader, ResourceId::new)?;
        let name_offset = reader.offset();
        let capability_name = CapabilityName::new(reader.read_str()?.to_owned())
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, name_offset))?;
        Ok(Self {
            action_id,
            actor_id,
            role,
            environment_id,
            resource_id,
            capability_name,
            operation_class: read_operation_class(reader)?,
            media_type: reader.read_str()?.to_owned(),
            payload: reader.read_bytes_owned()?,
        })
    }
}
