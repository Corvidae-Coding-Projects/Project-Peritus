//! Strict decoding for durable application-prompt rows.

use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, SessionId, WorkspaceId,
};
use rusqlite::Row;

use super::{
    ApplicationPromptId, ApplicationPromptRecord, ApplicationPromptSettlement,
    ApplicationPromptSettlementKind, ApplicationPromptState, ApplicationPromptTargetKind,
    ApplicationRequestId,
    prompt_types::{MAX_APPLICATION_PROMPT_BINDING_BYTES, MAX_APPLICATION_PROMPT_SETTLEMENT_BYTES},
};
use crate::{
    JournalError,
    sqlite::query::{array_from_blob, corrupt, digest_from_blob, positive_u64},
};

pub(super) struct PromptRow {
    prompt_id: Vec<u8>,
    actor_id: Vec<u8>,
    session_id: Vec<u8>,
    originating_request_id: Vec<u8>,
    target_kind: i64,
    acceptance_spec_id: Vec<u8>,
    harness_id: Vec<u8>,
    workspace_id: Vec<u8>,
    workspace_generation: i64,
    workspace_revision: i64,
    policy_id: Vec<u8>,
    provider_profile_id: Vec<u8>,
    freshness_digest: Vec<u8>,
    cancellation_generation: i64,
    binding_digest: Vec<u8>,
    binding_bytes: Vec<u8>,
    maximum_answer_bytes: i64,
    state: i64,
    settlement_kind: Option<i64>,
    settlement_request_id: Option<Vec<u8>>,
    settlement_digest: Option<Vec<u8>>,
    settlement_bytes: Option<Vec<u8>>,
}

impl PromptRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            prompt_id: row.get(0)?,
            actor_id: row.get(1)?,
            session_id: row.get(2)?,
            originating_request_id: row.get(3)?,
            target_kind: row.get(4)?,
            acceptance_spec_id: row.get(5)?,
            harness_id: row.get(6)?,
            workspace_id: row.get(7)?,
            workspace_generation: row.get(8)?,
            workspace_revision: row.get(9)?,
            policy_id: row.get(10)?,
            provider_profile_id: row.get(11)?,
            freshness_digest: row.get(12)?,
            cancellation_generation: row.get(13)?,
            binding_digest: row.get(14)?,
            binding_bytes: row.get(15)?,
            maximum_answer_bytes: row.get(16)?,
            state: row.get(17)?,
            settlement_kind: row.get(18)?,
            settlement_request_id: row.get(19)?,
            settlement_digest: row.get(20)?,
            settlement_bytes: row.get(21)?,
        })
    }

    pub(super) fn parse(self) -> Result<ApplicationPromptRecord, JournalError> {
        let revision = revision(&self)?;
        let state = ApplicationPromptState::from_tag(self.state)
            .ok_or_else(|| corrupt("unknown application prompt state"))?;
        let settlement = parse_settlement(
            self.settlement_kind,
            self.settlement_request_id,
            self.settlement_digest,
            self.settlement_bytes,
        )?;
        if (state == ApplicationPromptState::Awaiting) != settlement.is_none() {
            return Err(corrupt("stored application prompt settlement shape is inconsistent"));
        }
        if state == ApplicationPromptState::Answered
            && settlement
                .as_ref()
                .is_some_and(|value| value.kind() == ApplicationPromptSettlementKind::Cancellation)
        {
            return Err(corrupt("answered application prompt retains a cancellation settlement"));
        }
        if state == ApplicationPromptState::Cancelled
            && settlement
                .as_ref()
                .is_some_and(|value| value.kind() != ApplicationPromptSettlementKind::Cancellation)
        {
            return Err(corrupt("cancelled application prompt retains an answer settlement"));
        }
        let maximum_answer_bytes = usize::try_from(self.maximum_answer_bytes)
            .map_err(|_| corrupt("stored application prompt answer bound is invalid"))?;
        if maximum_answer_bytes == 0 || maximum_answer_bytes > 1024 * 1024 {
            return Err(corrupt("stored application prompt answer bound is outside limits"));
        }
        let binding_digest =
            digest_from_blob(&self.binding_digest, "application prompt binding digest")?;
        if self.binding_bytes.is_empty()
            || self.binding_bytes.len() > MAX_APPLICATION_PROMPT_BINDING_BYTES
            || peritus_codec::sha256(&self.binding_bytes) != binding_digest
        {
            return Err(corrupt("stored application prompt binding bytes are invalid"));
        }
        Ok(ApplicationPromptRecord {
            prompt_id: ApplicationPromptId::new(array_from_blob(
                &self.prompt_id,
                "application prompt identity",
            )?)
            .map_err(|_| corrupt("stored application prompt identity is invalid"))?,
            actor_id: ActorId::new(array_from_blob(&self.actor_id, "application prompt actor")?)
                .map_err(|_| corrupt("stored application prompt actor is invalid"))?,
            session_id: SessionId::new(array_from_blob(
                &self.session_id,
                "application prompt session",
            )?)
            .map_err(|_| corrupt("stored application prompt session is invalid"))?,
            originating_request_id: ApplicationRequestId::new(array_from_blob(
                &self.originating_request_id,
                "application prompt originating request",
            )?)
            .map_err(|_| corrupt("stored application prompt request identity is invalid"))?,
            target_kind: ApplicationPromptTargetKind::from_tag(self.target_kind)
                .ok_or_else(|| corrupt("unknown application prompt target kind"))?,
            revision,
            freshness_digest: digest_from_blob(
                &self.freshness_digest,
                "application prompt freshness digest",
            )?,
            cancellation_generation: Generation::new(positive_u64(
                self.cancellation_generation,
                "application prompt cancellation generation",
            )?)
            .map_err(|_| corrupt("stored application prompt cancellation generation is zero"))?,
            binding_digest,
            binding_bytes: self.binding_bytes,
            maximum_answer_bytes,
            state,
            settlement,
        })
    }
}

fn revision(row: &PromptRow) -> Result<RevisionTuple, JournalError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(array_from_blob(
            &row.acceptance_spec_id,
            "application prompt acceptance specification",
        )?)
        .map_err(|_| corrupt("stored prompt acceptance specification is invalid"))?,
        HarnessId::new(array_from_blob(&row.harness_id, "application prompt harness")?)
            .map_err(|_| corrupt("stored prompt harness is invalid"))?,
        WorkspaceId::new(array_from_blob(&row.workspace_id, "application prompt workspace")?)
            .map_err(|_| corrupt("stored prompt workspace is invalid"))?,
        Generation::new(positive_u64(
            row.workspace_generation,
            "application prompt workspace generation",
        )?)
        .map_err(|_| corrupt("stored prompt workspace generation is zero"))?,
        RevisionNumber::new(positive_u64(
            row.workspace_revision,
            "application prompt workspace revision",
        )?)
        .map_err(|_| corrupt("stored prompt workspace revision is zero"))?,
        PolicyId::new(array_from_blob(&row.policy_id, "application prompt policy")?)
            .map_err(|_| corrupt("stored prompt policy is invalid"))?,
        ProviderProfileId::new(array_from_blob(
            &row.provider_profile_id,
            "application prompt provider profile",
        )?)
        .map_err(|_| corrupt("stored prompt provider profile is invalid"))?,
    ))
}

fn parse_settlement(
    kind: Option<i64>,
    request_id: Option<Vec<u8>>,
    digest: Option<Vec<u8>>,
    bytes: Option<Vec<u8>>,
) -> Result<Option<ApplicationPromptSettlement>, JournalError> {
    let (kind, request_id, digest, bytes) = match (kind, request_id, digest, bytes) {
        (None, None, None, None) => return Ok(None),
        (Some(kind), Some(request_id), Some(digest), Some(bytes)) => {
            (kind, request_id, digest, bytes)
        }
        _ => return Err(corrupt("stored application prompt settlement is partial")),
    };
    if bytes.is_empty() || bytes.len() > MAX_APPLICATION_PROMPT_SETTLEMENT_BYTES {
        return Err(corrupt("stored application prompt settlement bytes are outside limits"));
    }
    let digest = digest_from_blob(&digest, "application prompt settlement digest")?;
    ApplicationPromptSettlement::new(
        ApplicationPromptSettlementKind::from_tag(kind)
            .ok_or_else(|| corrupt("unknown application prompt settlement kind"))?,
        ApplicationRequestId::new(array_from_blob(
            &request_id,
            "application prompt settlement request",
        )?)
        .map_err(|_| corrupt("stored prompt settlement request identity is invalid"))?,
        digest,
        bytes,
    )
    .map(Some)
    .map_err(|_| corrupt("stored application prompt settlement digest is invalid"))
}
