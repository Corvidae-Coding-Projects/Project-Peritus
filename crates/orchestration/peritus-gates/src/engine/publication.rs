//! Evidence publication from an exact durable passing-result observation.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{CommittedRecord, SqliteJournal};
use peritus_types::GateId;

use super::GateEngine;
use crate::wire::GateEventFrame;
use crate::{
    EvidencePublication, GateAttemptResult, GateError, GateErrorKind, GateEvidencePublisher,
    GateRecoveryAction, GateRejection, GateSlotPhase,
};

impl GateEngine {
    /// Publishes evidence for one exact durable passing-result record.
    ///
    /// # Errors
    /// Rejects any store identity, state, authoritative journal record, result, execution, or
    /// publisher receipt mismatch.
    pub fn publish_evidence(
        &self,
        journal: &SqliteJournal,
        gate_id: GateId,
        result_record: &CommittedRecord,
        publisher: &mut impl GateEvidencePublisher,
    ) -> Result<crate::GateEvidenceReceipt, GateError> {
        self.ensure_store(journal)?;
        self.validate_authoritative_result_record(journal, gate_id, result_record)?;
        let (attempt, result) = self.validate_result_record(gate_id, result_record)?;
        let planned = self.plan.gate(gate_id).ok_or_else(|| {
            crate::error::reject(
                GateRejection::IdentityMismatch,
                "evidence gate is absent from the exact plan",
            )
        })?;
        let publication = EvidencePublication::new(
            self.state.run_id(),
            gate_id,
            attempt,
            self.state.revision(),
            result_record.event_id(),
            result_record.global_position(),
            result.tool_result_digest(),
            planned.required_evidence().to_vec(),
            result.artifacts().to_vec(),
        )?;
        let receipt = publisher.publish(&publication)?;
        if receipt.publication() != &publication {
            return Err(crate::error::reject(
                GateRejection::EvidenceInvalid,
                "publisher receipt differs from the exact publication request",
            ));
        }
        Ok(receipt)
    }

    fn validate_authoritative_result_record(
        &self,
        journal: &SqliteJournal,
        gate_id: GateId,
        supplied: &CommittedRecord,
    ) -> Result<(), GateError> {
        let result_event =
            self.state.slot(gate_id).and_then(crate::GateSlot::result_event).ok_or_else(|| {
                crate::error::reject(
                    GateRejection::EvidenceInvalid,
                    "evidence gate has no durable result event",
                )
            })?;
        let aggregate = crate::gate_aggregate_key(self.state.run_id())?;
        let records = journal.records_for_aggregate(aggregate).map_err(|error| {
            GateError::sourced(
                GateErrorKind::Journal,
                GateRecoveryAction::ReplayAggregate,
                "authoritative gate result record could not be loaded",
                error,
            )
        })?;
        let authoritative = records.iter().find(|record| record.event_id() == result_event);
        if authoritative != Some(supplied) {
            return Err(crate::error::reject(
                GateRejection::EvidenceInvalid,
                "supplied result record differs from the authoritative gate journal",
            ));
        }
        Ok(())
    }

    fn validate_result_record<'a>(
        &'a self,
        gate_id: GateId,
        record: &CommittedRecord,
    ) -> Result<(crate::ActiveAttempt, &'a GateAttemptResult), GateError> {
        let slot = self.state.slot(gate_id).ok_or_else(|| {
            crate::error::reject(
                GateRejection::IdentityMismatch,
                "evidence gate is absent from authoritative state",
            )
        })?;
        let active = slot.active_attempt().ok_or_else(|| {
            crate::error::reject(
                GateRejection::IllegalTransition,
                "evidence-pending gate has no active attempt",
            )
        })?;
        let result = slot.last_result().ok_or_else(|| {
            crate::error::reject(
                GateRejection::EvidenceInvalid,
                "evidence-pending gate has no passing result",
            )
        })?;
        let decoded =
            decode_message::<GateEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|error| {
                    GateError::sourced(
                        GateErrorKind::Codec,
                        GateRecoveryAction::Quarantine,
                        "durable passing-result event cannot be decoded",
                        error,
                    )
                })?
                .into_event();
        let exact = matches!(
            decoded.kind(),
            crate::GateEventKind::ResultObserved {
                gate_id: observed_gate,
                execution_id,
                result: observed,
            } if *observed_gate == gate_id
                && *execution_id == active.execution_id()
                && observed == result
        );
        if slot.phase() != GateSlotPhase::EvidencePending
            || !result.passed()
            || record.aggregate() != crate::gate_aggregate_key(self.state.run_id())?
            || decoded.id() != record.event_id()
            || decoded.sequence() != record.sequence()
            || decoded.command_id() != record.command_id()
            || decoded.run_id() != self.state.run_id()
            || decoded.revision() != self.state.revision()
            || slot.result_event() != Some(record.event_id())
            || !exact
        {
            return Err(crate::error::reject(
                GateRejection::EvidenceInvalid,
                "C0 record is not the exact durable passing result for this gate",
            ));
        }
        Ok((active, result))
    }
}
