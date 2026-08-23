//! Canonical policy selectors, permissions, validity, and use bounds.

#![allow(
    clippy::missing_errors_doc,
    reason = "policy value codecs use the shared CodecError and checked PolicyError vocabularies"
)]

use super::dto::{PermissionDto, ScopeSelectorDto};
use crate::primitive::{read_id, read_revision, read_role, write_id, write_revision, write_role};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_policy::{
    ActorSelector, AuthorityInstant, EnvironmentSelector, Permission, PermissionSelector,
    PermissionSet, PolicyError, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{ActorId, CapabilityName, EnvironmentId, Generation, ResourceId};

pub fn write_permission(
    writer: &mut CanonicalWriter,
    value: &PermissionDto,
) -> Result<(), CodecError> {
    write_id(writer, value.resource_id.as_bytes())?;
    writer.write_str(value.capability_name.as_str())
}

pub fn read_permission(reader: &mut CanonicalReader<'_>) -> Result<PermissionDto, CodecError> {
    let resource_id = read_id(reader, ResourceId::new)?;
    let offset = reader.offset();
    let capability_name = CapabilityName::new(reader.read_str()?.to_owned())
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?;
    Ok(PermissionDto { resource_id, capability_name })
}

pub fn try_permission(value: PermissionDto) -> Permission {
    Permission::new(value.resource_id, value.capability_name)
}

pub fn write_validity(
    writer: &mut CanonicalWriter,
    value: ValidityWindow,
) -> Result<(), CodecError> {
    writer.write_u64(value.not_before().epoch().get())?;
    writer.write_u64(value.not_before().tick_millis())?;
    writer.write_u64(value.expires_at().epoch().get())?;
    writer.write_u64(value.expires_at().tick_millis())
}

pub fn read_validity(reader: &mut CanonicalReader<'_>) -> Result<ValidityWindow, CodecError> {
    let start_offset = reader.offset();
    let start_epoch = Generation::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, start_offset))?;
    let start_tick = reader.read_u64()?;
    let end_offset = reader.offset();
    let end_epoch = Generation::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, end_offset))?;
    let end_tick = reader.read_u64()?;
    ValidityWindow::new(
        AuthorityInstant::new(start_epoch, start_tick),
        AuthorityInstant::new(end_epoch, end_tick),
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, start_offset))
}

pub fn write_use_limit(writer: &mut CanonicalWriter, value: UseLimit) -> Result<(), CodecError> {
    writer.write_option_tag(value.remaining().is_some())?;
    if let Some(remaining) = value.remaining() {
        writer.write_u64(remaining)?;
    }
    Ok(())
}

pub fn read_use_limit(reader: &mut CanonicalReader<'_>) -> Result<UseLimit, CodecError> {
    if !reader.read_option_tag()? {
        return Ok(UseLimit::unlimited());
    }
    let offset = reader.offset();
    UseLimit::limited(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub fn write_selector(
    writer: &mut CanonicalWriter,
    value: &ScopeSelectorDto,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.actors.is_some())?;
    if let Some(values) = &value.actors {
        writer.write_collection_len(values.len())?;
        for value in values {
            write_id(writer, value.as_bytes())?;
        }
    }
    writer.write_option_tag(value.roles.is_some())?;
    if let Some(values) = &value.roles {
        writer.write_collection_len(values.len())?;
        for value in values {
            write_role(writer, *value)?;
        }
    }
    writer.write_option_tag(value.environments.is_some())?;
    if let Some(values) = &value.environments {
        writer.write_collection_len(values.len())?;
        for value in values {
            write_id(writer, value.as_bytes())?;
        }
    }
    writer.write_option_tag(value.permissions.is_some())?;
    if let Some(values) = &value.permissions {
        writer.write_collection_len(values.len())?;
        for value in values {
            writer.nested(|writer| write_permission(writer, value))?;
        }
    }
    write_revision(writer, &value.revision)
}

pub fn read_selector(reader: &mut CanonicalReader<'_>) -> Result<ScopeSelectorDto, CodecError> {
    let actors = if reader.read_option_tag()? {
        let count = reader.read_collection_len()?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(read_id(reader, ActorId::new)?);
        }
        Some(values)
    } else {
        None
    };
    let roles = if reader.read_option_tag()? {
        let count = reader.read_collection_len()?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(read_role(reader)?);
        }
        Some(values)
    } else {
        None
    };
    let environments = if reader.read_option_tag()? {
        let count = reader.read_collection_len()?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(read_id(reader, EnvironmentId::new)?);
        }
        Some(values)
    } else {
        None
    };
    let permissions = if reader.read_option_tag()? {
        let count = reader.read_collection_len()?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(reader.nested(read_permission)?);
        }
        Some(values)
    } else {
        None
    };
    Ok(ScopeSelectorDto {
        actors,
        roles,
        environments,
        permissions,
        revision: read_revision(reader)?,
    })
}

pub fn try_selector(value: ScopeSelectorDto) -> Result<ScopeSelector, PolicyError> {
    let actors = match value.actors {
        Some(values) => ActorSelector::exact(values)?,
        None => ActorSelector::any_within_parent(),
    };
    let roles = match value.roles {
        Some(values) => RoleSelector::exact(values)?,
        None => RoleSelector::any_within_parent(),
    };
    let environments = match value.environments {
        Some(values) => EnvironmentSelector::exact(values)?,
        None => EnvironmentSelector::any_within_parent(),
    };
    let permissions = match value.permissions {
        Some(values) => PermissionSelector::exact(PermissionSet::new(
            values.into_iter().map(try_permission).collect(),
        )?),
        None => PermissionSelector::any_within_parent(),
    };
    Ok(ScopeSelector::new(actors, roles, environments, permissions, value.revision))
}
