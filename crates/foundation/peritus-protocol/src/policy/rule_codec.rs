//! Canonical approval constraints, restriction rules, and layers.

#![allow(
    clippy::missing_errors_doc,
    reason = "policy rule codecs use the shared CodecError and checked PolicyError vocabularies"
)]

use super::dto::{
    ApprovalRequirementDto, RestrictionLayerDto, RestrictionRuleDto, RestrictionRuleKindDto,
};
use super::selector_codec::{
    read_selector, read_validity, try_selector, write_selector, write_validity,
};
use crate::primitive::{read_digest, read_role, write_digest, write_role};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_policy::{
    ApprovalRequirement, AuthorityTier, IndependenceRequirement, IndependenceSet, PolicyError,
    PolicyTier, RestrictionLayer, RestrictionRule,
};

pub fn write_approval(
    writer: &mut CanonicalWriter,
    value: &ApprovalRequirementDto,
) -> Result<(), CodecError> {
    writer.write_u16(authority_tier_tag(value.minimum_tier))?;
    writer.write_collection_len(value.approver_roles.len())?;
    for role in &value.approver_roles {
        write_role(writer, *role)?;
    }
    writer.write_collection_len(value.independence.len())?;
    for requirement in &value.independence {
        writer.write_u16(independence_tag(*requirement))?;
    }
    write_validity(writer, value.validity)
}

pub fn read_approval(
    reader: &mut CanonicalReader<'_>,
) -> Result<ApprovalRequirementDto, CodecError> {
    let minimum_tier = read_authority_tier(reader)?;
    let role_count = reader.read_collection_len()?;
    let mut approver_roles = Vec::with_capacity(role_count);
    for _ in 0..role_count {
        approver_roles.push(read_role(reader)?);
    }
    let independence_count = reader.read_collection_len()?;
    let mut independence = Vec::with_capacity(independence_count);
    for _ in 0..independence_count {
        independence.push(read_independence(reader)?);
    }
    Ok(ApprovalRequirementDto {
        minimum_tier,
        approver_roles,
        independence,
        validity: read_validity(reader)?,
    })
}

pub fn try_approval(value: ApprovalRequirementDto) -> Result<ApprovalRequirement, PolicyError> {
    ApprovalRequirement::new(
        value.minimum_tier,
        value.approver_roles,
        IndependenceSet::new(value.independence)?,
        value.validity,
    )
}

pub fn write_rule(
    writer: &mut CanonicalWriter,
    value: &RestrictionRuleDto,
) -> Result<(), CodecError> {
    write_digest(writer, &value.digest)?;
    writer.nested(|writer| write_selector(writer, &value.selector))?;
    match &value.kind {
        RestrictionRuleKindDto::Deny => writer.write_u16(1),
        RestrictionRuleKindDto::RequireApproval(requirement) => {
            writer.write_u16(2)?;
            writer.nested(|writer| write_approval(writer, requirement))
        }
    }
}

pub fn read_rule(reader: &mut CanonicalReader<'_>) -> Result<RestrictionRuleDto, CodecError> {
    let digest = read_digest(reader)?;
    let selector = reader.nested(read_selector)?;
    let offset = reader.offset();
    let kind = match reader.read_u16()? {
        1 => RestrictionRuleKindDto::Deny,
        2 => RestrictionRuleKindDto::RequireApproval(reader.nested(read_approval)?),
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    };
    Ok(RestrictionRuleDto { digest, selector, kind })
}

pub fn try_rule(value: RestrictionRuleDto) -> Result<RestrictionRule, PolicyError> {
    let selector = try_selector(value.selector)?;
    match value.kind {
        RestrictionRuleKindDto::Deny => Ok(RestrictionRule::deny(value.digest, selector)),
        RestrictionRuleKindDto::RequireApproval(requirement) => Ok(
            RestrictionRule::require_approval(value.digest, selector, try_approval(requirement)?),
        ),
    }
}

pub fn write_layer(
    writer: &mut CanonicalWriter,
    value: &RestrictionLayerDto,
) -> Result<(), CodecError> {
    writer.write_u16(policy_tier_tag(value.tier))?;
    writer.write_collection_len(value.rules.len())?;
    for rule in &value.rules {
        writer.nested(|writer| write_rule(writer, rule))?;
    }
    Ok(())
}

pub fn read_layer(reader: &mut CanonicalReader<'_>) -> Result<RestrictionLayerDto, CodecError> {
    let tier = read_policy_tier(reader)?;
    let count = reader.read_collection_len()?;
    let mut rules = Vec::with_capacity(count);
    for _ in 0..count {
        rules.push(reader.nested(read_rule)?);
    }
    Ok(RestrictionLayerDto { tier, rules })
}

pub fn try_layer(value: RestrictionLayerDto) -> Result<RestrictionLayer, PolicyError> {
    RestrictionLayer::new(
        value.tier,
        value.rules.into_iter().map(try_rule).collect::<Result<Vec<_>, _>>()?,
    )
}

pub const fn policy_tier_tag(tier: PolicyTier) -> u16 {
    match tier {
        PolicyTier::System => 1,
        PolicyTier::User => 2,
        PolicyTier::Project => 3,
        PolicyTier::Run => 4,
        PolicyTier::Session => 5,
        PolicyTier::RoleHarness => 6,
    }
}

pub fn read_policy_tier(reader: &mut CanonicalReader<'_>) -> Result<PolicyTier, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(PolicyTier::System),
        2 => Ok(PolicyTier::User),
        3 => Ok(PolicyTier::Project),
        4 => Ok(PolicyTier::Run),
        5 => Ok(PolicyTier::Session),
        6 => Ok(PolicyTier::RoleHarness),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn authority_tier_tag(tier: AuthorityTier) -> u16 {
    match tier {
        AuthorityTier::Project => 1,
        AuthorityTier::User => 2,
        AuthorityTier::Organization => 3,
        AuthorityTier::System => 4,
    }
}

fn read_authority_tier(reader: &mut CanonicalReader<'_>) -> Result<AuthorityTier, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(AuthorityTier::Project),
        2 => Ok(AuthorityTier::User),
        3 => Ok(AuthorityTier::Organization),
        4 => Ok(AuthorityTier::System),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn independence_tag(value: IndependenceRequirement) -> u16 {
    match value {
        IndependenceRequirement::NotRequester => 1,
        IndependenceRequirement::NotActionActor => 2,
        IndependenceRequirement::NoProducingAttemptParticipation => 3,
        IndependenceRequirement::NoReviewParticipation => 4,
    }
}

fn read_independence(
    reader: &mut CanonicalReader<'_>,
) -> Result<IndependenceRequirement, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(IndependenceRequirement::NotRequester),
        2 => Ok(IndependenceRequirement::NotActionActor),
        3 => Ok(IndependenceRequirement::NoProducingAttemptParticipation),
        4 => Ok(IndependenceRequirement::NoReviewParticipation),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
