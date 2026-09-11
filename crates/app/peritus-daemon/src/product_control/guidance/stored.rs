//! Canonical guidance storage DTOs; field order and serde representation are stable.

use super::{Error, GuidanceCatalog, STORAGE_SCHEMA, stored_app};
use peritus_app_protocol::{
    ControlOperationId, ConversationId, WorkbenchGuidanceContent, WorkbenchGuidanceIdentity,
    WorkbenchGuidancePrior, WorkbenchGuidanceReason, WorkbenchGuidanceRecord,
    WorkbenchGuidanceScope, WorkbenchGuidanceSource, WorkbenchGuidanceText,
    WorkbenchGuidanceTombstone, WorkbenchGuidanceValidation, WorkbenchGuidanceVersion,
    WorkbenchInvocationId,
};
use peritus_product_runner::control::ControlError;
use peritus_types::{Sha256Digest, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredCatalog {
    pub(super) schema: u16,
    pub(super) workspace: [u8; 16],
    pub(super) revision: u64,
    pub(super) identities: Vec<[u8; 16]>,
}

impl StoredCatalog {
    pub(super) fn from_domain(value: &GuidanceCatalog) -> Self {
        Self {
            schema: STORAGE_SCHEMA,
            workspace: *value.workspace.as_bytes(),
            revision: value.revision,
            identities: value.identities.iter().map(|id| *id.as_bytes()).collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredSlot {
    pub(super) schema: u16,
    pub(super) identity: [u8; 16],
    pub(super) workspace: [u8; 16],
    pub(super) record_revision: u64,
    pub(super) dependency_revision: u64,
    pub(super) state: StoredSlotState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "lifecycle", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum StoredSlotState {
    Active {
        text: String,
        source: StoredSource,
        scope: StoredScope,
        pinned: bool,
        validated_by: [u8; 16],
        validation_digest: [u8; 32],
    },
    Forgotten {
        tombstone_digest: [u8; 32],
    },
}

impl StoredSlot {
    pub(super) fn active(record: &WorkbenchGuidanceRecord) -> Self {
        Self {
            schema: STORAGE_SCHEMA,
            identity: *record.identity().id().as_bytes(),
            workspace: *record.identity().workspace().as_bytes(),
            record_revision: record.version().record(),
            dependency_revision: record.version().dependency(),
            state: StoredSlotState::Active {
                text: record.content().text().as_str().to_owned(),
                source: StoredSource::from_public(record.content().source()),
                scope: StoredScope::from_public(record.content().scope()),
                pinned: record.pinned(),
                validated_by: *record.last_validation().operation().as_bytes(),
                validation_digest: record.last_validation().content_digest().into_bytes(),
            },
        }
    }

    pub(super) fn forgotten(
        tombstone: &WorkbenchGuidanceTombstone,
        digest: Sha256Digest,
    ) -> Result<Self, Error> {
        Ok(Self {
            schema: STORAGE_SCHEMA,
            identity: *tombstone.identity().id().as_bytes(),
            workspace: *tombstone.identity().workspace().as_bytes(),
            record_revision: tombstone
                .prior()
                .revision()
                .checked_add(1)
                .ok_or(ControlError::Capacity)?,
            dependency_revision: tombstone.dependency_revision(),
            state: StoredSlotState::Forgotten { tombstone_digest: digest.into_bytes() },
        })
    }

    pub(super) fn validate_identity(
        &self,
        workspace: WorkspaceId,
        identity: ControlOperationId,
        revision: u64,
    ) -> Result<(), Error> {
        if self.schema != STORAGE_SCHEMA
            || self.workspace != *workspace.as_bytes()
            || self.identity != *identity.as_bytes()
            || self.record_revision == 0
            || self.record_revision != revision
            || self.dependency_revision == 0
        {
            return Err(Error::Corrupt("invalid guidance slot"));
        }
        Ok(())
    }

    pub(super) fn into_active(self) -> Result<WorkbenchGuidanceRecord, Error> {
        let StoredSlotState::Active {
            text,
            source,
            scope,
            pinned,
            validated_by,
            validation_digest,
        } = self.state
        else {
            return Err(Error::Corrupt("forgotten guidance is not active"));
        };
        let identity = stored_identity(self.identity, self.workspace)?;
        let content = stored_app(WorkbenchGuidanceContent::new(
            stored_app(WorkbenchGuidanceText::new(text))?,
            source.into_public()?,
            scope.into_public()?,
        ))?;
        stored_app(WorkbenchGuidanceRecord::new(
            identity,
            stored_app(WorkbenchGuidanceVersion::new(
                self.record_revision,
                self.dependency_revision,
            ))?,
            content,
            pinned,
            WorkbenchGuidanceValidation::new(
                stored_operation(validated_by)?,
                Sha256Digest::new(validation_digest),
            ),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum StoredSource {
    UserAuthored,
    AcceptedPublicReply { operation: [u8; 16], invocation: [u8; 16], digest: [u8; 32] },
}

impl StoredSource {
    const fn from_public(value: WorkbenchGuidanceSource) -> Self {
        match value {
            WorkbenchGuidanceSource::UserAuthored => Self::UserAuthored,
            WorkbenchGuidanceSource::AcceptedPublicReply { operation, invocation, digest } => {
                Self::AcceptedPublicReply {
                    operation: operation.into_bytes(),
                    invocation: invocation.into_bytes(),
                    digest: digest.into_bytes(),
                }
            }
        }
    }

    pub(super) fn into_public(self) -> Result<WorkbenchGuidanceSource, Error> {
        Ok(match self {
            Self::UserAuthored => WorkbenchGuidanceSource::UserAuthored,
            Self::AcceptedPublicReply { operation, invocation, digest } => {
                WorkbenchGuidanceSource::AcceptedPublicReply {
                    operation: stored_operation(operation)?,
                    invocation: WorkbenchInvocationId::new(invocation)
                        .map_err(|_| Error::Corrupt("invalid guidance invocation"))?,
                    digest: Sha256Digest::new(digest),
                }
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum StoredScope {
    Project,
    Conversation { conversation: [u8; 16] },
}

impl StoredScope {
    const fn from_public(value: WorkbenchGuidanceScope) -> Self {
        match value {
            WorkbenchGuidanceScope::Project => Self::Project,
            WorkbenchGuidanceScope::Conversation(conversation) => {
                Self::Conversation { conversation: conversation.into_bytes() }
            }
        }
    }

    pub(super) fn into_public(self) -> Result<WorkbenchGuidanceScope, Error> {
        match self {
            Self::Project => Ok(WorkbenchGuidanceScope::Project),
            Self::Conversation { conversation } => Ok(WorkbenchGuidanceScope::Conversation(
                ConversationId::new(conversation)
                    .map_err(|_| Error::Corrupt("invalid guidance conversation"))?,
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredTombstone {
    pub(super) schema: u16,
    pub(super) identity: [u8; 16],
    pub(super) workspace: [u8; 16],
    pub(super) prior_revision: u64,
    pub(super) prior_digest: [u8; 32],
    pub(super) prior_scope: StoredScope,
    pub(super) prior_pinned: bool,
    pub(super) forgotten_by: [u8; 16],
    pub(super) reason: String,
    pub(super) dependency_revision: u64,
}

impl StoredTombstone {
    pub(super) fn from_public(value: &WorkbenchGuidanceTombstone) -> Self {
        Self {
            schema: STORAGE_SCHEMA,
            identity: *value.identity().id().as_bytes(),
            workspace: *value.identity().workspace().as_bytes(),
            prior_revision: value.prior().revision(),
            prior_digest: value.prior().digest().into_bytes(),
            prior_scope: StoredScope::from_public(value.prior().scope()),
            prior_pinned: value.prior().pinned(),
            forgotten_by: *value.forgotten_by().as_bytes(),
            reason: value.reason().as_str().to_owned(),
            dependency_revision: value.dependency_revision(),
        }
    }

    pub(super) fn into_public(self) -> Result<WorkbenchGuidanceTombstone, Error> {
        if self.schema != STORAGE_SCHEMA {
            return Err(Error::Corrupt("unsupported guidance tombstone schema"));
        }
        stored_app(WorkbenchGuidanceTombstone::new(
            stored_identity(self.identity, self.workspace)?,
            stored_app(WorkbenchGuidancePrior::new(
                self.prior_revision,
                Sha256Digest::new(self.prior_digest),
                self.prior_scope.into_public()?,
                self.prior_pinned,
            ))?,
            stored_operation(self.forgotten_by)?,
            stored_app(WorkbenchGuidanceReason::new(self.reason))?,
            self.dependency_revision,
        ))
    }
}

fn stored_identity(
    identity: [u8; 16],
    workspace: [u8; 16],
) -> Result<WorkbenchGuidanceIdentity, Error> {
    Ok(WorkbenchGuidanceIdentity::new(
        stored_operation(identity)?,
        WorkspaceId::new(workspace).map_err(|_| Error::Corrupt("invalid guidance workspace"))?,
    ))
}

fn stored_operation(bytes: [u8; 16]) -> Result<ControlOperationId, Error> {
    ControlOperationId::new(bytes).map_err(|_| Error::Corrupt("invalid guidance operation"))
}
