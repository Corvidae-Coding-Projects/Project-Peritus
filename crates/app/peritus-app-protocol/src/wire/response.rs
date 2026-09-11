//! Canonical schema-v1 application response family.

use crate::{
    APP_SCHEMA_V1, AppProtocolLimits, AppResponseEnvelope, AppResponsePayload, CorrelationId,
    EventCursor, OperationAcknowledgement, PromptId, RESPONSE_FAMILY, RequestId, SubscriptionId,
    SubscriptionStarted,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use super::{
    artifact::{read_artifact_metadata, write_artifact_metadata},
    command::{read_command_result, write_command_result},
    daemon::{
        read_daemon_status, read_shutdown_accepted, write_daemon_status, write_shutdown_accepted,
    },
    error::{read_app_error, write_app_error},
    primitive::{read_context, read_id, unknown, write_context, write_id},
    product::{
        read_conversation, read_settlement_snapshot, read_settlement_snapshots, read_snapshot,
        read_snapshots, write_conversation, write_settlement_snapshot, write_settlement_snapshots,
        write_snapshot, write_snapshots,
    },
    terminal::{read_terminal_binding, write_terminal_binding},
};

impl CanonicalEncode for AppResponseEnvelope {
    const FAMILY: u16 = RESPONSE_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        validate_response_binding(self, writer.len())?;
        write_context(writer, self.context())?;
        write_id(writer, self.request_id().as_bytes())?;
        write_id(writer, self.correlation_id().as_bytes())?;
        writer.write_u16(payload_tag(self.payload()))?;
        match self.payload() {
            AppResponsePayload::WorkbenchCheckpoint(value) => {
                super::workbench_checkpoints::write_checkpoint_receipt(writer, value)
            }
            AppResponsePayload::WorkbenchRewindPreview(value) => {
                super::workbench_checkpoints::write_preview(writer, value)
            }
            AppResponsePayload::WorkbenchRestore(value) => {
                super::workbench_checkpoints::write_restore_receipt(writer, value)
            }
            AppResponsePayload::WorkbenchMemory(value) => {
                super::workbench_memory::write_memory(writer, value)
            }
            AppResponsePayload::InitProposal(value) => {
                super::workbench_init::write_proposal(writer, value)
            }
            AppResponsePayload::WorkbenchPermissions(value) => {
                super::workbench_permissions::write_permissions(writer, value)
            }
            AppResponsePayload::WorkbenchCompactionPreview(value) => {
                super::workbench_compaction::write_preview(writer, value)
            }
            AppResponsePayload::ConversationLibrary(value) => {
                super::workbench_library::write_page(writer, value)
            }
            AppResponsePayload::WorkbenchGoal(value) => {
                super::workbench_goal::write_snapshot(writer, value)
            }
            AppResponsePayload::WorkbenchResult(value) => {
                super::workbench_launch::write_page(writer, value)
            }
            AppResponsePayload::WorkbenchReview(value) => {
                super::workbench_review::write_page(writer, value)
            }
            AppResponsePayload::WorkbenchFileImportPreview(value) => {
                super::workbench_files::write_import_preview(writer, value)
            }
            AppResponsePayload::WorkbenchImages(value) => {
                super::workbench_image_page::write_page(writer, value)
            }
            AppResponsePayload::WorkbenchImagePreview(value) => {
                super::workbench_images::write_preview(writer, value)
            }
            AppResponsePayload::WorkbenchFilePreview(value) => {
                super::workbench_files::write_preview(writer, value)
            }
            AppResponsePayload::WorkbenchFiles(value) => {
                super::workbench_files::write_page(writer, value)
            }
            AppResponsePayload::WorkbenchBrief(value) => {
                super::workbench_brief::write_brief(writer, value)
            }
            AppResponsePayload::WorkbenchContext(value) => {
                super::workbench_context::write_page(writer, value)
            }
            AppResponsePayload::WorkbenchQueue(value) => {
                super::workbench_inputs::write_page(writer, value)
            }
            AppResponsePayload::Workbench(value) => super::workbench::write_snapshot(writer, value),
            AppResponsePayload::WorkbenchReceipt(value) => {
                super::workbench::write_receipt(writer, value)
            }
            AppResponsePayload::Doctor(value) => super::doctor::write_report(writer, value),
            AppResponsePayload::Interaction(value) => {
                super::interaction::write_snapshot(writer, value)
            }
            AppResponsePayload::Models(value) => super::interaction::write_catalog(writer, value),
            AppResponsePayload::CommandResult(value) => write_command_result(writer, value),
            AppResponsePayload::SubscriptionStarted(value) => {
                write_subscription_started(writer, *value)
            }
            AppResponsePayload::ArtifactOpened(value) => write_artifact_metadata(writer, value),
            AppResponsePayload::PromptAccepted(value) => write_id(writer, value.as_bytes()),
            AppResponsePayload::TerminalAttached(value) => write_terminal_binding(writer, *value),
            AppResponsePayload::Acknowledged(value) => {
                write_id(writer, value.request_id().as_bytes())
            }
            AppResponsePayload::DaemonStatus(value) => write_daemon_status(writer, value),
            AppResponsePayload::ShutdownAccepted(value) => write_shutdown_accepted(writer, *value),
            AppResponsePayload::Error(value) => write_app_error(writer, value),
            AppResponsePayload::ProductRunAccepted(value) => write_snapshot(writer, value),
            AppResponsePayload::ProductRuns(value) => write_snapshots(writer, value),
            AppResponsePayload::ProductRunConversation(value) => write_conversation(writer, value),
            AppResponsePayload::ProductRunSettled(value) => {
                write_settlement_snapshot(writer, value)
            }
            AppResponsePayload::ProductRunSettlements(value) => {
                write_settlement_snapshots(writer, value)
            }
        }
    }
}

fn payload_tag(payload: &AppResponsePayload) -> u16 {
    match payload {
        AppResponsePayload::WorkbenchCheckpoint(_) => 120,
        AppResponsePayload::WorkbenchRewindPreview(_) => 121,
        AppResponsePayload::WorkbenchRestore(_) => 122,
        AppResponsePayload::WorkbenchMemory(_) => 161,
        AppResponsePayload::InitProposal(_) => 162,
        AppResponsePayload::WorkbenchPermissions(_) => 160,
        AppResponsePayload::WorkbenchCompactionPreview(_) => 29,
        AppResponsePayload::WorkbenchResult(_) => 100,
        AppResponsePayload::WorkbenchReview(_) => 80,
        AppResponsePayload::ConversationLibrary(_) => 140,
        AppResponsePayload::CommandResult(_) => 1,
        AppResponsePayload::SubscriptionStarted(_) => 2,
        AppResponsePayload::ArtifactOpened(_) => 3,
        AppResponsePayload::PromptAccepted(_) => 4,
        AppResponsePayload::TerminalAttached(_) => 5,
        AppResponsePayload::Acknowledged(_) => 6,
        AppResponsePayload::DaemonStatus(_) => 7,
        AppResponsePayload::ShutdownAccepted(_) => 8,
        AppResponsePayload::Error(_) => 9,
        AppResponsePayload::ProductRunAccepted(_) => 10,
        AppResponsePayload::ProductRuns(_) => 11,
        AppResponsePayload::ProductRunConversation(_) => 12,
        AppResponsePayload::ProductRunSettled(_) => 13,
        AppResponsePayload::ProductRunSettlements(_) => 14,
        AppResponsePayload::Interaction(value) => {
            if value.models().has_effort() {
                17
            } else {
                15
            }
        }
        AppResponsePayload::Models(_) => 16,
        AppResponsePayload::Doctor(_) => 18,
        AppResponsePayload::Workbench(_) => 19,
        AppResponsePayload::WorkbenchReceipt(_) => 20,
        AppResponsePayload::WorkbenchQueue(_) => 21,
        AppResponsePayload::WorkbenchContext(_) => 22,
        AppResponsePayload::WorkbenchBrief(_) => 23,
        AppResponsePayload::WorkbenchImagePreview(_) => 24,
        AppResponsePayload::WorkbenchImages(_) => 25,
        AppResponsePayload::WorkbenchFilePreview(_) => 26,
        AppResponsePayload::WorkbenchFileImportPreview(_) => 28,
        AppResponsePayload::WorkbenchFiles(_) => 27,
        AppResponsePayload::WorkbenchGoal(_) => 60,
    }
}

impl CanonicalDecode for AppResponseEnvelope {
    const FAMILY: u16 = RESPONSE_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_response(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_response(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<AppResponseEnvelope, CodecError> {
    let offset = reader.offset();
    let context = read_context(reader)?;
    let request_id = read_id(reader, RequestId::new)?;
    let correlation_id = read_id(reader, CorrelationId::new)?;
    let tag_offset = reader.offset();
    let payload = match reader.read_u16()? {
        100 => AppResponsePayload::WorkbenchResult(super::workbench_launch::read_page(reader)?),
        80 => AppResponsePayload::WorkbenchReview(super::workbench_review::read_page(reader)?),
        120 => AppResponsePayload::WorkbenchCheckpoint(
            super::workbench_checkpoints::read_checkpoint_receipt(reader)?,
        ),
        121 => AppResponsePayload::WorkbenchRewindPreview(
            super::workbench_checkpoints::read_preview(reader)?,
        ),
        122 => AppResponsePayload::WorkbenchRestore(
            super::workbench_checkpoints::read_restore_receipt(reader)?,
        ),
        140 => {
            AppResponsePayload::ConversationLibrary(super::workbench_library::read_page(reader)?)
        }
        161 => AppResponsePayload::WorkbenchMemory(super::workbench_memory::read_memory(reader)?),
        162 => AppResponsePayload::InitProposal(super::workbench_init::read_proposal(reader)?),
        160 => AppResponsePayload::WorkbenchPermissions(
            super::workbench_permissions::read_permissions(reader)?,
        ),
        1 => AppResponsePayload::CommandResult(read_command_result(reader, limits)?),
        2 => AppResponsePayload::SubscriptionStarted(read_subscription_started(reader, limits)?),
        3 => AppResponsePayload::ArtifactOpened(read_artifact_metadata(reader, limits)?),
        4 => AppResponsePayload::PromptAccepted(read_id(reader, PromptId::new)?),
        5 => AppResponsePayload::TerminalAttached(read_terminal_binding(reader)?),
        6 => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(read_id(
            reader,
            RequestId::new,
        )?)),
        7 => AppResponsePayload::DaemonStatus(read_daemon_status(reader, limits)?),
        8 => AppResponsePayload::ShutdownAccepted(read_shutdown_accepted(reader)?),
        9 => AppResponsePayload::Error(read_app_error(reader, limits)?),
        10 => AppResponsePayload::ProductRunAccepted(read_snapshot(reader)?),
        11 => AppResponsePayload::ProductRuns(read_snapshots(reader)?),
        12 => AppResponsePayload::ProductRunConversation(read_conversation(reader)?),
        13 => AppResponsePayload::ProductRunSettled(read_settlement_snapshot(reader)?),
        14 => AppResponsePayload::ProductRunSettlements(read_settlement_snapshots(reader)?),
        15 => AppResponsePayload::Interaction(super::interaction::read_snapshot(reader, false)?),
        16 => AppResponsePayload::Models(super::interaction::read_catalog(reader)?),
        17 => AppResponsePayload::Interaction(super::interaction::read_snapshot(reader, true)?),
        18 => AppResponsePayload::Doctor(super::doctor::read_report(reader)?),
        19 => AppResponsePayload::Workbench(super::workbench::read_snapshot(reader)?),
        20 => AppResponsePayload::WorkbenchReceipt(super::workbench::read_receipt(reader)?),
        21 => AppResponsePayload::WorkbenchQueue(super::workbench_inputs::read_page(reader)?),
        22 => AppResponsePayload::WorkbenchContext(super::workbench_context::read_page(reader)?),
        23 => AppResponsePayload::WorkbenchBrief(super::workbench_brief::read_brief(reader)?),
        28 => AppResponsePayload::WorkbenchFileImportPreview(
            super::workbench_files::read_import_preview(reader)?,
        ),
        29 => AppResponsePayload::WorkbenchCompactionPreview(
            super::workbench_compaction::read_preview(reader)?,
        ),
        26 => {
            AppResponsePayload::WorkbenchFilePreview(super::workbench_files::read_preview(reader)?)
        }
        27 => AppResponsePayload::WorkbenchFiles(super::workbench_files::read_page(reader)?),
        25 => AppResponsePayload::WorkbenchImages(super::workbench_image_page::read_page(reader)?),
        24 => AppResponsePayload::WorkbenchImagePreview(super::workbench_images::read_preview(
            reader,
        )?),
        60 => AppResponsePayload::WorkbenchGoal(super::workbench_goal::read_snapshot(reader)?),
        _ => return unknown(tag_offset),
    };
    let response = AppResponseEnvelope::new(context, request_id, correlation_id, payload);
    validate_response_binding(&response, offset)?;
    Ok(response)
}

fn write_subscription_started(
    writer: &mut CanonicalWriter,
    value: SubscriptionStarted,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    writer.write_u64(value.after().get())?;
    writer.write_u32(value.maximum_in_flight())
}

fn read_subscription_started(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<SubscriptionStarted, CodecError> {
    let offset = reader.offset();
    let subscription_id = read_id(reader, SubscriptionId::new)?;
    let after = EventCursor::new(reader.read_u64()?);
    let maximum = reader.read_u32()?;
    let maximum_usize = usize::try_from(maximum)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))?;
    if maximum_usize > limits.max_in_flight_events() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    if maximum == 0 {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(SubscriptionStarted::new(subscription_id, after, maximum))
}

fn validate_response_binding(
    response: &AppResponseEnvelope,
    offset: usize,
) -> Result<(), CodecError> {
    let matches = match response.payload() {
        AppResponsePayload::CommandResult(value) => {
            value.original_request_id() == response.request_id()
        }
        AppResponsePayload::TerminalAttached(value) => {
            value.originating_request_id() == response.request_id()
        }
        AppResponsePayload::Acknowledged(value) => value.request_id() == response.request_id(),
        AppResponsePayload::ShutdownAccepted(value) => {
            value.request().request_id() == response.request_id()
                && value.request().correlation_id() == response.correlation_id()
        }
        _ => true,
    };
    if matches { Ok(()) } else { Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset)) }
}
