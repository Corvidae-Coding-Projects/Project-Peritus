//! Bounded Workbench inspection request payload encodings.
use crate::AppRequestPayload;
use peritus_codec::{CanonicalWriter, CodecError};
pub(super) fn write(
    writer: &mut CanonicalWriter,
    payload: &AppRequestPayload,
) -> Option<Result<(), CodecError>> {
    Some(match payload {
        AppRequestPayload::QueryWorkbenchBriefPage(value) => {
            crate::wire::workbench_brief_pages::write_request(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchBriefProposal(value) => {
            crate::wire::workbench_brief_pages::write_proposal_request(writer, *value)
        }
        AppRequestPayload::PreviewWorkbenchRewind(value)
        | AppRequestPayload::InspectWorkbenchCheckpoint(value) => {
            super::super::workbench_checkpoints::write_request(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchCheckpointPage(value) => {
            super::super::workbench_checkpoint_pages::write_checkpoint_request(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchRewindPage(value) => {
            super::super::workbench_checkpoint_pages::write_rewind_request(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchMemory(value) => {
            super::super::workbench_memory::write_query(writer, *value)
        }
        AppRequestPayload::DiscoverInitArtifacts(value) => {
            crate::wire::workbench_init_artifacts::write_discovery(writer, value)
        }
        AppRequestPayload::QueryInitArtifactPage(value) => {
            crate::wire::workbench_init_artifacts::write_page_request(writer, *value)
        }
        AppRequestPayload::DiscoverInit(value) => {
            super::super::workbench_init::write_discovery_request(writer, *value)
        }
        AppRequestPayload::PreviewWorkbenchCompaction(value) => {
            super::super::workbench_compaction::write_request(writer, value)
        }
        AppRequestPayload::QueryWorkbenchResult(value)
        | AppRequestPayload::QueryWorkbenchPreview(value) => {
            super::super::workbench_launch::write_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchPreviewOutput(value) => {
            super::super::workbench_launch::write_output_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchReview(value)
        | AppRequestPayload::QueryWorkbenchReviewSummary(value) => {
            super::super::workbench_review::write_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchReviewDiff(value) => {
            super::super::workbench_review::write_diff_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchReviewDiffBytes(value) => {
            super::super::workbench_review::write_diff_bytes_query(writer, *value)
        }
        AppRequestPayload::QueryConversationLibrary(value) => {
            super::super::workbench_library::write_query(writer, value)
        }
        AppRequestPayload::BeginWorkbenchFileUpload(value) => {
            super::super::workbench_files::write_upload(writer, value)
        }
        AppRequestPayload::PreviewWorkbenchFileImport(value) => {
            super::super::workbench_files::write_import_request(writer, value)
        }
        AppRequestPayload::QueryWorkbenchImages(value) => {
            super::super::workbench_image_page::write_query(writer, *value)
        }
        AppRequestPayload::BeginWorkbenchImageUpload(value) => {
            super::super::workbench_images::write_upload(writer, value)
        }
        AppRequestPayload::PreviewWorkbenchImage(value) => {
            super::super::workbench_images::write_request(writer, value)
        }
        AppRequestPayload::PreviewWorkbenchFile(value) => {
            super::super::workbench_files::write_request(writer, value)
        }
        AppRequestPayload::QueryWorkbenchFiles(value) => {
            super::super::workbench_files::write_query(writer, *value)
        }
        AppRequestPayload::QueryWorkbenchContext(value) => {
            super::super::workbench_context::write_query(writer, *value)
        }
        _ => return None,
    })
}
