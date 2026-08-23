//! Digest-bound canonical single-tier policy amendments.

#![allow(
    clippy::missing_errors_doc,
    reason = "amendment codecs use the shared CodecError and checked PolicyError vocabularies"
)]

use super::dto::RestrictionLayerDto;
use super::rule_codec::{policy_tier_tag, read_layer, read_policy_tier, try_layer, write_layer};
use crate::SCHEMA_V1;
use crate::primitive::{read_digest, read_id, write_digest, write_id};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits, canonical_sha256,
};
use peritus_policy::{PolicyAmendmentProposal, PolicyError, PolicyTier};
use peritus_types::{PolicyId, Sha256Digest};

#[derive(Clone, Debug, Eq, PartialEq)]
struct PolicyAmendmentContent {
    base_policy_id: PolicyId,
    successor_policy_id: PolicyId,
    tier: PolicyTier,
    replacement: RestrictionLayerDto,
}

/// Complete amendment data with a digest over an exact content-only canonical frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyAmendmentDto {
    /// Immutable base policy identity.
    pub base_policy_id: PolicyId,
    /// Fresh successor policy identity.
    pub successor_policy_id: PolicyId,
    /// Sole replaced restriction tier.
    pub tier: PolicyTier,
    /// Complete replacement layer.
    pub replacement: RestrictionLayerDto,
    /// SHA-256 of the family-22 content frame.
    pub amendment_digest: Sha256Digest,
}

impl PolicyAmendmentDto {
    /// Builds a proposal and computes its digest over all non-self-referential content.
    pub fn new(
        base_policy_id: PolicyId,
        successor_policy_id: PolicyId,
        tier: PolicyTier,
        replacement: RestrictionLayerDto,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        let content = PolicyAmendmentContent {
            base_policy_id,
            successor_policy_id,
            tier,
            replacement: replacement.clone(),
        };
        let amendment_digest = canonical_sha256(&content, limits)?;
        Ok(Self { base_policy_id, successor_policy_id, tier, replacement, amendment_digest })
    }

    /// Verifies the digest and reconstructs the checked unprivileged amendment proposal.
    pub fn try_into_domain(
        self,
        limits: CodecLimits,
    ) -> Result<PolicyAmendmentProposal, PolicyAmendmentConversionError> {
        self.verify_digest(limits).map_err(PolicyAmendmentConversionError::Codec)?;
        let replacement =
            try_layer(self.replacement).map_err(PolicyAmendmentConversionError::Policy)?;
        PolicyAmendmentProposal::new(
            self.base_policy_id,
            self.successor_policy_id,
            self.tier,
            replacement,
            self.amendment_digest,
        )
        .map_err(PolicyAmendmentConversionError::Policy)
    }

    /// Imports a domain proposal only when its supplied digest matches canonical content bytes.
    pub fn try_from_domain(
        value: &PolicyAmendmentProposal,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        let result = Self {
            base_policy_id: value.base_policy_id(),
            successor_policy_id: value.successor_policy_id(),
            tier: value.tier(),
            replacement: value.replacement().into(),
            amendment_digest: value.amendment_digest(),
        };
        result.verify_digest(limits)?;
        Ok(result)
    }

    fn content(&self) -> PolicyAmendmentContent {
        PolicyAmendmentContent {
            base_policy_id: self.base_policy_id,
            successor_policy_id: self.successor_policy_id,
            tier: self.tier,
            replacement: self.replacement.clone(),
        }
    }

    fn verify_digest(&self, limits: CodecLimits) -> Result<(), CodecError> {
        if canonical_sha256(&self.content(), limits)? == self.amendment_digest {
            Ok(())
        } else {
            Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0))
        }
    }
}

/// Checked amendment conversion failure without treating bytes as authenticated authority.
#[derive(Debug)]
pub enum PolicyAmendmentConversionError {
    /// Canonical digest or encoding failure.
    Codec(CodecError),
    /// Checked B1 proposal validation failure.
    Policy(PolicyError),
}

impl CanonicalEncode for PolicyAmendmentContent {
    const FAMILY: u16 = 22;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_content(writer, self)
    }
}

impl CanonicalEncode for PolicyAmendmentDto {
    const FAMILY: u16 = 23;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_content(writer, &self.content())?;
        write_digest(writer, &self.amendment_digest)
    }
}

impl CanonicalDecode for PolicyAmendmentDto {
    const FAMILY: u16 = 23;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let start = reader.offset();
        let content = read_content(reader)?;
        let value = Self {
            base_policy_id: content.base_policy_id,
            successor_policy_id: content.successor_policy_id,
            tier: content.tier,
            replacement: content.replacement,
            amendment_digest: read_digest(reader)?,
        };
        value
            .clone()
            .try_into_domain(CodecLimits::PRODUCTION)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, start))?;
        Ok(value)
    }
}

fn write_content(
    writer: &mut CanonicalWriter,
    value: &PolicyAmendmentContent,
) -> Result<(), CodecError> {
    write_id(writer, value.base_policy_id.as_bytes())?;
    write_id(writer, value.successor_policy_id.as_bytes())?;
    writer.write_u16(policy_tier_tag(value.tier))?;
    writer.nested(|writer| write_layer(writer, &value.replacement))
}

fn read_content(reader: &mut CanonicalReader<'_>) -> Result<PolicyAmendmentContent, CodecError> {
    Ok(PolicyAmendmentContent {
        base_policy_id: read_id(reader, PolicyId::new)?,
        successor_policy_id: read_id(reader, PolicyId::new)?,
        tier: read_policy_tier(reader)?,
        replacement: reader.nested(read_layer)?,
    })
}
