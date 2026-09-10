//! Canonical schema-v1 application request family.

use crate::{
    APP_SCHEMA_V1, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, CorrelationId,
    REQUEST_FAMILY, RequestId,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

mod support;

use support::{
    read_artifact_open, read_subscription_request, write_artifact_open, write_subscription_request,
};

use super::{
    artifact::{
        read_artifact_cancellation, read_artifact_chunk, read_artifact_completion,
        read_artifact_metadata, write_artifact_cancellation, write_artifact_chunk,
        write_artifact_completion, write_artifact_metadata,
    },
    command::{read_command_binding, write_command_binding},
    daemon::{read_shutdown_request, write_shutdown_request},
    primitive::{invalid, read_context, read_id, unknown, write_context, write_id},
    product::{
        read_conversation_query, read_run_continuation, read_run_control, read_run_query,
        read_run_request, write_conversation_query, write_run_continuation, write_run_control,
        write_run_query, write_run_request,
    },
    prompt::{
        read_prompt_answer, read_prompt_cancellation, write_prompt_answer,
        write_prompt_cancellation,
    },
    terminal::{
        read_terminal_binding, read_terminal_cancellation, read_terminal_detach,
        read_terminal_input, read_terminal_resize, write_terminal_binding,
        write_terminal_cancellation, write_terminal_detach, write_terminal_input,
        write_terminal_resize,
    },
};

impl CanonicalEncode for AppRequestEnvelope {
    const FAMILY: u16 = REQUEST_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        validate_request_binding(self, writer.len())?;
        write_context(writer, self.context())?;
        write_id(writer, self.request_id().as_bytes())?;
        write_id(writer, self.correlation_id().as_bytes())?;
        write_payload(writer, self.payload())
    }
}

fn write_payload(
    writer: &mut CanonicalWriter,
    payload: &AppRequestPayload,
) -> Result<(), CodecError> {
    writer.write_u16(payload_tag(payload))?;
    match payload {
        AppRequestPayload::PreviewWorkbenchRewind(value)
        | AppRequestPayload::InspectWorkbenchCheckpoint(value) => {
            super::workbench_checkpoints::write_request(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchMemory(value) => {
            super::workbench_memory::write_query(writer, *value)
        }
        AppRequestPayload::DiscoverInit(value) => {
            super::workbench_init::write_discovery_request(writer, *value)
        }
        AppRequestPayload::PreviewWorkbenchCompaction(value) => {
            super::workbench_compaction::write_request(writer, value)
        }
        AppRequestPayload::QueryWorkbenchResult(value) => {
            super::workbench_launch::write_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchReview(value) => {
            super::workbench_review::write_query(writer, *value)
        }
        AppRequestPayload::QueryConversationLibrary(value) => {
            super::workbench_library::write_query(writer, value)
        }
        AppRequestPayload::BeginWorkbenchFileUpload(value) => {
            super::workbench_files::write_upload(writer, value)
        }
        AppRequestPayload::PreviewWorkbenchFileImport(value) => {
            super::workbench_files::write_import_request(writer, value)
        }
        AppRequestPayload::QueryWorkbenchImages(value) => {
            super::workbench_image_page::write_query(writer, *value)
        }
        AppRequestPayload::BeginWorkbenchImageUpload(value) => {
            super::workbench_images::write_upload(writer, value)
        }
        AppRequestPayload::PreviewWorkbenchImage(value) => {
            super::workbench_images::write_request(writer, value)
        }
        AppRequestPayload::PreviewWorkbenchFile(value) => {
            super::workbench_files::write_request(writer, value)
        }
        AppRequestPayload::QueryWorkbenchFiles(value) => {
            super::workbench_files::write_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchContext(value) => {
            super::workbench_context::write_query(writer, *value)
        }
        AppRequestPayload::WorkbenchCommand(value)
        | AppRequestPayload::QueryWorkbenchReceipt(value) => {
            super::workbench::write_command(writer, value)
        }
        AppRequestPayload::QueryWorkbench(value)
        | AppRequestPayload::QueryWorkbenchPermissions(value)
        | AppRequestPayload::QueryWorkbenchBrief(value)
        | AppRequestPayload::QueryWorkbenchGoal(value) => {
            super::workbench::write_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchQueue(value) => {
            super::workbench_inputs::write_query(writer, *value)
        }
        AppRequestPayload::Doctor(value) => super::doctor::write_query(writer, *value),
        AppRequestPayload::Interact(value) => super::interaction::write_request(writer, value),
        AppRequestPayload::QueryInteraction(value) => write_conversation_query(writer, *value),
        AppRequestPayload::QueryModels(value) => {
            super::interaction::write_model_query(writer, *value)
        }
        AppRequestPayload::SubmitCommand(value) => write_command_binding(writer, value),
        AppRequestPayload::Subscribe(value) => write_subscription_request(writer, value),
        AppRequestPayload::OpenArtifact(value) => write_artifact_open(writer, *value),
        AppRequestPayload::CancelArtifact(value) => write_artifact_cancellation(writer, *value),
        AppRequestPayload::AnswerPrompt(value) => write_prompt_answer(writer, value),
        AppRequestPayload::CancelPrompt(value) => write_prompt_cancellation(writer, *value),
        AppRequestPayload::AttachTerminal(value) => write_terminal_binding(writer, *value),
        AppRequestPayload::TerminalInput(value) => write_terminal_input(writer, value),
        AppRequestPayload::TerminalResize(value) => write_terminal_resize(writer, *value),
        AppRequestPayload::DetachTerminal(value) => write_terminal_detach(writer, *value),
        AppRequestPayload::CancelTerminal(value) => write_terminal_cancellation(writer, *value),
        AppRequestPayload::DaemonStatus => Ok(()),
        AppRequestPayload::Shutdown(value) => write_shutdown_request(writer, *value),
        AppRequestPayload::BeginArtifactUpload(value) => write_artifact_metadata(writer, value),
        AppRequestPayload::UploadArtifactChunk(value) => write_artifact_chunk(writer, value),
        AppRequestPayload::CompleteArtifactUpload(value) => {
            write_artifact_completion(writer, *value)
        }
        AppRequestPayload::StartProductRun(value) => write_run_request(writer, value),
        AppRequestPayload::ControlProductRun(value) => write_run_control(writer, *value),
        AppRequestPayload::QueryProductRuns(value) => write_run_query(writer, *value),
        AppRequestPayload::ContinueProductRun(value) => write_run_continuation(writer, value),
        AppRequestPayload::UpdateModels(value) => {
            super::interaction::write_model_update(writer, value)
        }
        AppRequestPayload::QueryProductRunConversation(value) => {
            write_conversation_query(writer, *value)
        }
    }
}

fn payload_tag(payload: &AppRequestPayload) -> u16 {
    match payload {
        AppRequestPayload::PreviewWorkbenchRewind(_) => 120,
        AppRequestPayload::InspectWorkbenchCheckpoint(_) => 121,
        AppRequestPayload::QueryWorkbenchMemory(_) => 161,
        AppRequestPayload::DiscoverInit(_) => 162,
        AppRequestPayload::QueryWorkbenchPermissions(_) => 160,
        AppRequestPayload::PreviewWorkbenchCompaction(_) => 42,
        AppRequestPayload::QueryWorkbenchResult(_) => 100,
        AppRequestPayload::QueryWorkbenchReview(_) => 80,
        AppRequestPayload::QueryConversationLibrary(_) => 140,
        AppRequestPayload::QueryWorkbenchImages(_) => 37,
        AppRequestPayload::BeginWorkbenchImageUpload(_) => 35,
        AppRequestPayload::PreviewWorkbenchImage(_) => 36,
        AppRequestPayload::PreviewWorkbenchFile(_) => 38,
        AppRequestPayload::BeginWorkbenchFileUpload(_) => 40,
        AppRequestPayload::PreviewWorkbenchFileImport(_) => 41,
        AppRequestPayload::QueryWorkbenchFiles(_) => 39,
        AppRequestPayload::QueryWorkbenchContext(_) => 33,
        AppRequestPayload::QueryWorkbenchBrief(_) => 34,
        AppRequestPayload::WorkbenchCommand(_) => 29,
        AppRequestPayload::QueryWorkbench(_) => 30,
        AppRequestPayload::QueryWorkbenchReceipt(_) => 31,
        AppRequestPayload::QueryWorkbenchQueue(_) => 32,
        AppRequestPayload::SubmitCommand(_) => 1,
        AppRequestPayload::Subscribe(_) => 2,
        AppRequestPayload::OpenArtifact(_) => 3,
        AppRequestPayload::CancelArtifact(_) => 4,
        AppRequestPayload::AnswerPrompt(_) => 5,
        AppRequestPayload::CancelPrompt(_) => 6,
        AppRequestPayload::AttachTerminal(_) => 7,
        AppRequestPayload::TerminalInput(_) => 8,
        AppRequestPayload::TerminalResize(_) => 9,
        AppRequestPayload::DetachTerminal(_) => 10,
        AppRequestPayload::CancelTerminal(_) => 11,
        AppRequestPayload::DaemonStatus => 12,
        AppRequestPayload::Shutdown(_) => 13,
        AppRequestPayload::BeginArtifactUpload(_) => 14,
        AppRequestPayload::UploadArtifactChunk(_) => 15,
        AppRequestPayload::CompleteArtifactUpload(_) => 16,
        AppRequestPayload::StartProductRun(_) => 17,
        AppRequestPayload::ControlProductRun(_) => 18,
        AppRequestPayload::QueryProductRuns(_) => 19,
        AppRequestPayload::ContinueProductRun(_) => 20,
        AppRequestPayload::QueryProductRunConversation(_) => 21,
        AppRequestPayload::Interact(value) => {
            if value.models().has_effort() {
                26
            } else {
                22
            }
        }
        AppRequestPayload::QueryInteraction(_) => 23,
        AppRequestPayload::QueryModels(_) => 24,
        AppRequestPayload::UpdateModels(value) => {
            if value.models().has_effort() {
                27
            } else {
                25
            }
        }
        AppRequestPayload::Doctor(_) => 28,
        AppRequestPayload::QueryWorkbenchGoal(_) => 60,
    }
}

impl CanonicalDecode for AppRequestEnvelope {
    const FAMILY: u16 = REQUEST_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_request(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_request(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<AppRequestEnvelope, CodecError> {
    let offset = reader.offset();
    let context = read_context(reader)?;
    let request_id = read_id(reader, RequestId::new)?;
    let correlation_id = read_id(reader, CorrelationId::new)?;
    let tag_offset = reader.offset();
    let payload = match reader.read_u16()? {
        100 => {
            AppRequestPayload::QueryWorkbenchResult(super::workbench_launch::read_query(reader)?)
        }
        80 => AppRequestPayload::QueryWorkbenchReview(super::workbench_review::read_query(reader)?),
        120 => AppRequestPayload::PreviewWorkbenchRewind(
            super::workbench_checkpoints::read_request(reader)?,
        ),
        121 => AppRequestPayload::InspectWorkbenchCheckpoint(
            super::workbench_checkpoints::read_request(reader)?,
        ),
        140 => AppRequestPayload::QueryConversationLibrary(super::workbench_library::read_query(
            reader,
        )?),
        161 => {
            AppRequestPayload::QueryWorkbenchMemory(super::workbench_memory::read_query(reader)?)
        }
        162 => {
            AppRequestPayload::DiscoverInit(super::workbench_init::read_discovery_request(reader)?)
        }
        160 => AppRequestPayload::QueryWorkbenchPermissions(super::workbench::read_query(reader)?),
        1 => AppRequestPayload::SubmitCommand(read_command_binding(reader, limits)?),
        2 => AppRequestPayload::Subscribe(read_subscription_request(reader, limits)?),
        3 => AppRequestPayload::OpenArtifact(read_artifact_open(reader)?),
        4 => AppRequestPayload::CancelArtifact(read_artifact_cancellation(reader)?),
        5 => AppRequestPayload::AnswerPrompt(read_prompt_answer(reader, limits)?),
        6 => AppRequestPayload::CancelPrompt(read_prompt_cancellation(reader)?),
        7 => AppRequestPayload::AttachTerminal(read_terminal_binding(reader)?),
        8 => AppRequestPayload::TerminalInput(read_terminal_input(reader, limits)?),
        9 => AppRequestPayload::TerminalResize(read_terminal_resize(reader)?),
        10 => AppRequestPayload::DetachTerminal(read_terminal_detach(reader)?),
        11 => AppRequestPayload::CancelTerminal(read_terminal_cancellation(reader)?),
        12 => AppRequestPayload::DaemonStatus,
        13 => AppRequestPayload::Shutdown(read_shutdown_request(reader)?),
        14 => AppRequestPayload::BeginArtifactUpload(read_artifact_metadata(reader, limits)?),
        15 => AppRequestPayload::UploadArtifactChunk(read_artifact_chunk(reader, limits)?),
        16 => AppRequestPayload::CompleteArtifactUpload(read_artifact_completion(reader)?),
        17 => AppRequestPayload::StartProductRun(read_run_request(reader)?),
        18 => AppRequestPayload::ControlProductRun(read_run_control(reader)?),
        19 => AppRequestPayload::QueryProductRuns(read_run_query(reader)?),
        20 => AppRequestPayload::ContinueProductRun(read_run_continuation(reader)?),
        21 => AppRequestPayload::QueryProductRunConversation(read_conversation_query(reader)?),
        22 => AppRequestPayload::Interact(super::interaction::read_request(reader, false)?),
        23 => AppRequestPayload::QueryInteraction(read_conversation_query(reader)?),
        24 => AppRequestPayload::QueryModels(super::interaction::read_model_query(reader)?),
        25 => {
            AppRequestPayload::UpdateModels(super::interaction::read_model_update(reader, false)?)
        }
        26 => AppRequestPayload::Interact(super::interaction::read_request(reader, true)?),
        27 => AppRequestPayload::UpdateModels(super::interaction::read_model_update(reader, true)?),
        28 => AppRequestPayload::Doctor(super::doctor::read_query(reader)?),
        60 => AppRequestPayload::QueryWorkbenchGoal(super::workbench::read_query(reader)?),
        29 => AppRequestPayload::WorkbenchCommand(super::workbench::read_command(reader)?),
        30 => AppRequestPayload::QueryWorkbench(super::workbench::read_query(reader)?),
        31 => AppRequestPayload::QueryWorkbenchReceipt(super::workbench::read_command(reader)?),
        32 => AppRequestPayload::QueryWorkbenchQueue(super::workbench_inputs::read_query(reader)?),
        34 => AppRequestPayload::QueryWorkbenchBrief(super::workbench::read_query(reader)?),
        38 => {
            AppRequestPayload::PreviewWorkbenchFile(super::workbench_files::read_request(reader)?)
        }
        39 => AppRequestPayload::QueryWorkbenchFiles(super::workbench_files::read_query(reader)?),
        40 => AppRequestPayload::BeginWorkbenchFileUpload(super::workbench_files::read_upload(
            reader, limits,
        )?),
        41 => AppRequestPayload::PreviewWorkbenchFileImport(
            super::workbench_files::read_import_request(reader)?,
        ),
        42 => AppRequestPayload::PreviewWorkbenchCompaction(
            super::workbench_compaction::read_request(reader)?,
        ),
        37 => AppRequestPayload::QueryWorkbenchImages(super::workbench_image_page::read_query(
            reader,
        )?),
        35 => AppRequestPayload::BeginWorkbenchImageUpload(super::workbench_images::read_upload(
            reader, limits,
        )?),
        36 => {
            AppRequestPayload::PreviewWorkbenchImage(super::workbench_images::read_request(reader)?)
        }
        33 => {
            AppRequestPayload::QueryWorkbenchContext(super::workbench_context::read_query(reader)?)
        }
        _ => return unknown(tag_offset),
    };
    let request =
        invalid(offset, AppRequestEnvelope::new(context, request_id, correlation_id, payload))?;
    validate_request_binding(&request, offset)?;
    Ok(request)
}

fn validate_request_binding(value: &AppRequestEnvelope, offset: usize) -> Result<(), CodecError> {
    let matches = match value.payload() {
        AppRequestPayload::AttachTerminal(inner) => {
            inner.originating_request_id() == value.request_id()
        }
        AppRequestPayload::CancelArtifact(inner) => {
            inner.correlation_id() == value.correlation_id()
        }
        AppRequestPayload::AnswerPrompt(inner) => {
            inner.correlation().session_id() == value.context().session_id()
        }
        AppRequestPayload::CancelPrompt(inner) => {
            if inner.correlation_id() == value.correlation_id() {
                inner.correlation().session_id() == value.context().session_id()
            } else {
                false
            }
        }
        AppRequestPayload::DetachTerminal(inner) => {
            inner.correlation_id() == value.correlation_id()
        }
        AppRequestPayload::CancelTerminal(inner) => {
            inner.correlation_id() == value.correlation_id()
        }
        _ => true,
    };
    if matches { Ok(()) } else { Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset)) }
}
