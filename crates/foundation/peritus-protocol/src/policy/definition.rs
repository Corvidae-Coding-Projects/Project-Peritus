//! Complete canonical checked policy definitions.

#![allow(
    clippy::missing_errors_doc,
    reason = "policy definition codecs use the shared CodecError and checked PolicyError vocabularies"
)]

use super::dto::{
    AuthorityBoundaryDto, AuthorityCeilingDto, CeilingGrantDto, OperationDescriptorDto,
    RestrictionLayerDto,
};
use super::rule_codec::{read_layer, read_rule, try_layer, try_rule, write_layer, write_rule};
use super::selector_codec::{
    read_selector, read_use_limit, read_validity, try_permission, try_selector, write_selector,
    write_use_limit, write_validity,
};
use super::tags::{operation_class_tag, read_operation_class};
use crate::SCHEMA_V1;
use crate::primitive::{
    read_digest, read_id, read_revision, read_role, write_digest, write_id, write_revision,
    write_role,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_policy::{
    AuthorityBoundary, AuthorityCeiling, CeilingGrant, OperationDescriptor, OperationRegistry,
    PermissionSet, PolicyDefinition, PolicyError, RiskClass, RiskSet,
};
use peritus_types::{ActorId, CapabilityName, EnvironmentId, PolicyId};

/// Complete stable DTO for one immutable B1 policy definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyDefinitionDto {
    /// Sole immutable policy identity.
    pub policy_id: PolicyId,
    /// Protected authority ceiling.
    pub ceiling: AuthorityCeilingDto,
    /// Canonical operation registry.
    pub operations: Vec<OperationDescriptorDto>,
    /// Strict tier-ordered lower restrictions.
    pub layers: Vec<RestrictionLayerDto>,
}

impl PolicyDefinitionDto {
    /// Reconstructs a fully checked, authority-neutral policy definition.
    pub fn try_into_domain(self) -> Result<PolicyDefinition, PolicyError> {
        let boundary = try_boundary(self.ceiling.boundary)?;
        let grants =
            self.ceiling.grants.into_iter().map(try_grant).collect::<Result<Vec<_>, _>>()?;
        let denies = self
            .ceiling
            .immutable_denies
            .into_iter()
            .map(try_rule)
            .collect::<Result<Vec<_>, _>>()?;
        let ceiling = AuthorityCeiling::new(boundary, grants, denies)?;
        let operations = OperationRegistry::new(
            self.operations.into_iter().map(try_operation).collect::<Result<Vec<_>, _>>()?,
        )?;
        let layers = self.layers.into_iter().map(try_layer).collect::<Result<Vec<_>, _>>()?;
        PolicyDefinition::new(self.policy_id, ceiling, operations, layers)
    }
}

impl From<&PolicyDefinition> for PolicyDefinitionDto {
    fn from(value: &PolicyDefinition) -> Self {
        Self {
            policy_id: value.policy_id(),
            ceiling: value.ceiling().into(),
            operations: value
                .operations()
                .as_slice()
                .iter()
                .map(OperationDescriptorDto::from)
                .collect(),
            layers: value.layers().iter().map(RestrictionLayerDto::from).collect(),
        }
    }
}

impl CanonicalEncode for PolicyDefinitionDto {
    const FAMILY: u16 = 21;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.policy_id.as_bytes())?;
        writer.nested(|writer| write_ceiling(writer, &self.ceiling))?;
        writer.write_collection_len(self.operations.len())?;
        for operation in &self.operations {
            writer.nested(|writer| write_operation(writer, operation))?;
        }
        writer.write_collection_len(self.layers.len())?;
        for layer in &self.layers {
            writer.nested(|writer| write_layer(writer, layer))?;
        }
        Ok(())
    }
}

impl CanonicalDecode for PolicyDefinitionDto {
    const FAMILY: u16 = 21;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let start = reader.offset();
        let policy_id = read_id(reader, PolicyId::new)?;
        let ceiling = reader.nested(read_ceiling)?;
        let operation_count = reader.read_collection_len()?;
        let mut operations = Vec::with_capacity(operation_count);
        for _ in 0..operation_count {
            operations.push(reader.nested(read_operation)?);
        }
        let layer_count = reader.read_collection_len()?;
        let mut layers = Vec::with_capacity(layer_count);
        for _ in 0..layer_count {
            layers.push(reader.nested(read_layer)?);
        }
        let value = Self { policy_id, ceiling, operations, layers };
        value
            .clone()
            .try_into_domain()
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, start))?;
        Ok(value)
    }
}

fn write_boundary(
    writer: &mut CanonicalWriter,
    value: &AuthorityBoundaryDto,
) -> Result<(), CodecError> {
    writer.write_collection_len(value.actors.len())?;
    for id in &value.actors {
        write_id(writer, id.as_bytes())?;
    }
    writer.write_collection_len(value.roles.len())?;
    for role in &value.roles {
        write_role(writer, *role)?;
    }
    writer.write_collection_len(value.environments.len())?;
    for id in &value.environments {
        write_id(writer, id.as_bytes())?;
    }
    writer.write_collection_len(value.permissions.len())?;
    for permission in &value.permissions {
        writer.nested(|writer| super::selector_codec::write_permission(writer, permission))?;
    }
    write_revision(writer, &value.revision)?;
    write_validity(writer, value.validity)?;
    write_use_limit(writer, value.use_limit)
}

fn read_boundary(reader: &mut CanonicalReader<'_>) -> Result<AuthorityBoundaryDto, CodecError> {
    let actor_count = reader.read_collection_len()?;
    let mut actors = Vec::with_capacity(actor_count);
    for _ in 0..actor_count {
        actors.push(read_id(reader, ActorId::new)?);
    }
    let role_count = reader.read_collection_len()?;
    let mut roles = Vec::with_capacity(role_count);
    for _ in 0..role_count {
        roles.push(read_role(reader)?);
    }
    let environment_count = reader.read_collection_len()?;
    let mut environments = Vec::with_capacity(environment_count);
    for _ in 0..environment_count {
        environments.push(read_id(reader, EnvironmentId::new)?);
    }
    let permission_count = reader.read_collection_len()?;
    let mut permissions = Vec::with_capacity(permission_count);
    for _ in 0..permission_count {
        permissions.push(reader.nested(super::selector_codec::read_permission)?);
    }
    Ok(AuthorityBoundaryDto {
        actors,
        roles,
        environments,
        permissions,
        revision: read_revision(reader)?,
        validity: read_validity(reader)?,
        use_limit: read_use_limit(reader)?,
    })
}

fn try_boundary(value: AuthorityBoundaryDto) -> Result<AuthorityBoundary, PolicyError> {
    AuthorityBoundary::new(
        value.actors,
        value.roles,
        value.environments,
        PermissionSet::new(value.permissions.into_iter().map(try_permission).collect())?,
        value.revision,
        value.validity,
        value.use_limit,
    )
}

fn write_grant(writer: &mut CanonicalWriter, value: &CeilingGrantDto) -> Result<(), CodecError> {
    write_digest(writer, &value.digest)?;
    writer.nested(|writer| write_selector(writer, &value.selector))?;
    write_validity(writer, value.validity)?;
    write_use_limit(writer, value.use_limit)
}

fn read_grant(reader: &mut CanonicalReader<'_>) -> Result<CeilingGrantDto, CodecError> {
    Ok(CeilingGrantDto {
        digest: read_digest(reader)?,
        selector: reader.nested(read_selector)?,
        validity: read_validity(reader)?,
        use_limit: read_use_limit(reader)?,
    })
}

fn try_grant(value: CeilingGrantDto) -> Result<CeilingGrant, PolicyError> {
    Ok(CeilingGrant::new(
        value.digest,
        try_selector(value.selector)?,
        value.validity,
        value.use_limit,
    ))
}

fn write_ceiling(
    writer: &mut CanonicalWriter,
    value: &AuthorityCeilingDto,
) -> Result<(), CodecError> {
    writer.nested(|writer| write_boundary(writer, &value.boundary))?;
    writer.write_collection_len(value.grants.len())?;
    for grant in &value.grants {
        writer.nested(|writer| write_grant(writer, grant))?;
    }
    writer.write_collection_len(value.immutable_denies.len())?;
    for rule in &value.immutable_denies {
        writer.nested(|writer| write_rule(writer, rule))?;
    }
    Ok(())
}

fn read_ceiling(reader: &mut CanonicalReader<'_>) -> Result<AuthorityCeilingDto, CodecError> {
    let boundary = reader.nested(read_boundary)?;
    let grant_count = reader.read_collection_len()?;
    let mut grants = Vec::with_capacity(grant_count);
    for _ in 0..grant_count {
        grants.push(reader.nested(read_grant)?);
    }
    let deny_count = reader.read_collection_len()?;
    let mut immutable_denies = Vec::with_capacity(deny_count);
    for _ in 0..deny_count {
        immutable_denies.push(reader.nested(read_rule)?);
    }
    Ok(AuthorityCeilingDto { boundary, grants, immutable_denies })
}

fn write_operation(
    writer: &mut CanonicalWriter,
    value: &OperationDescriptorDto,
) -> Result<(), CodecError> {
    writer.write_str(value.name.as_str())?;
    writer.write_u16(operation_class_tag(value.operation_class))?;
    writer.write_collection_len(value.risks.len())?;
    for risk in &value.risks {
        writer.write_u16(risk_tag(*risk))?;
    }
    Ok(())
}

fn read_operation(reader: &mut CanonicalReader<'_>) -> Result<OperationDescriptorDto, CodecError> {
    let name_offset = reader.offset();
    let name = CapabilityName::new(reader.read_str()?.to_owned())
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, name_offset))?;
    let operation_class = read_operation_class(reader)?;
    let risk_count = reader.read_collection_len()?;
    let mut risks = Vec::with_capacity(risk_count);
    for _ in 0..risk_count {
        risks.push(read_risk(reader)?);
    }
    Ok(OperationDescriptorDto { name, operation_class, risks })
}

fn try_operation(value: OperationDescriptorDto) -> Result<OperationDescriptor, PolicyError> {
    OperationDescriptor::new(value.name, value.operation_class, RiskSet::new(value.risks)?)
}

const fn risk_tag(risk: RiskClass) -> u16 {
    match risk {
        RiskClass::Read => 1,
        RiskClass::ScopedWrite => 2,
        RiskClass::Execution => 3,
        RiskClass::Network => 4,
        RiskClass::DependencyEnvironment => 5,
        RiskClass::RepositoryHistoryMutation => 6,
        RiskClass::SecretUse => 7,
        RiskClass::ExternalSideEffect => 8,
        RiskClass::PolicyAuthority => 9,
        RiskClass::HarnessPromotion => 10,
    }
}

fn read_risk(reader: &mut CanonicalReader<'_>) -> Result<RiskClass, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(RiskClass::Read),
        2 => Ok(RiskClass::ScopedWrite),
        3 => Ok(RiskClass::Execution),
        4 => Ok(RiskClass::Network),
        5 => Ok(RiskClass::DependencyEnvironment),
        6 => Ok(RiskClass::RepositoryHistoryMutation),
        7 => Ok(RiskClass::SecretUse),
        8 => Ok(RiskClass::ExternalSideEffect),
        9 => Ok(RiskClass::PolicyAuthority),
        10 => Ok(RiskClass::HarnessPromotion),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
