//! Bounded control-safe quality result rendering from authenticated output artifacts.

use peritus_artifact_store::{ArtifactDigest, ArtifactReadHandle, ArtifactStore};
use peritus_process::{OutputArtifact, TerminalResult};
use peritus_tool_protocol::{BoundedText, Truncation, render_output_tail};
use peritus_tool_router::DispatchFailure;

use crate::{
    dispatcher::adapter_failure,
    error::truncate_utf8,
    execution::failure,
};

pub(crate) struct RenderedOutput {
    pub(crate) model: BoundedText,
    pub(crate) human: BoundedText,
    pub(crate) model_truncation: Truncation,
    pub(crate) human_truncation: Truncation,
}

struct ArtifactSource {
    artifact: OutputArtifact,
    reader: ArtifactReadHandle,
    separator_before: bool,
    logical_start: u64,
}

/// Reads only the suffix needed by the larger requested rendering from authenticated artifacts.
///
/// One additional raw byte makes an omitted prefix observable to `render_output_tail`, including
/// when every selected byte occupies one display byte. Model and human renderings are then derived
/// independently from the same exact canonical stdout, stderr, terminal page.
pub(crate) fn output(
    store: &ArtifactStore,
    terminal: &TerminalResult,
    model_limit: u32,
    human_limit: u32,
) -> Result<RenderedOutput, DispatchFailure> {
    let page = output_page(store, terminal, model_limit.max(human_limit))?;
    let (model, model_truncation) = render_output_tail(&page, model_limit);
    let (human, human_truncation) = render_output_tail(&page, human_limit);
    Ok(RenderedOutput { model, human, model_truncation, human_truncation })
}

fn output_page(
    store: &ArtifactStore,
    terminal: &TerminalResult,
    maximum: u32,
) -> Result<Vec<u8>, DispatchFailure> {
    let page_capacity = u64::from(maximum.max(1)).checked_add(1).ok_or_else(|| {
        render_failure("quality rendering page capacity overflowed")
    })?;
    let mut sources = Vec::with_capacity(terminal.artifacts().len());
    let mut total = 0_u64;
    let mut has_prior = false;
    let mut prior_ended_with_newline = false;

    for artifact in terminal.artifacts().iter().copied() {
        if artifact.start_offset() != 0 || artifact.end_offset() != artifact.size() {
            return Err(render_failure(
                "quality output artifact range differs from its authenticated object",
            ));
        }
        if artifact.size() == 0 {
            continue;
        }
        let digest = ArtifactDigest::from_sha256(artifact.digest());
        let mut reader = store.open_read(digest).map_err(|error| failure::artifact(&error))?;
        if reader.metadata().digest() != digest || reader.metadata().size() != artifact.size() {
            return Err(render_failure(
                "opened quality output artifact differs from terminal evidence",
            ));
        }
        let last_offset = artifact.size().checked_sub(1).ok_or_else(|| {
            render_failure("nonempty quality output artifact has no final byte")
        })?;
        let last = reader
            .read_chunk_at(last_offset, 1)
            .map_err(|error| failure::artifact(&error))?
            .ok_or_else(|| render_failure("quality output artifact ended before its final byte"))?;
        if last.offset() != last_offset || last.bytes().len() != 1 {
            return Err(render_failure(
                "quality output artifact returned an inexact final-byte page",
            ));
        }
        let separator_before = has_prior && !prior_ended_with_newline;
        if separator_before {
            total = total.checked_add(1).ok_or_else(|| {
                render_failure("quality rendering separator accounting overflowed")
            })?;
        }
        let logical_start = total;
        total = total.checked_add(artifact.size()).ok_or_else(|| {
            render_failure("quality rendering source accounting overflowed")
        })?;
        prior_ended_with_newline = last.bytes()[0] == b'\n';
        has_prior = true;
        sources.push(ArtifactSource {
            artifact,
            reader,
            separator_before,
            logical_start,
        });
    }

    let page_start = total.saturating_sub(page_capacity);
    let expected = usize::try_from(total - page_start).map_err(|_| {
        render_failure("quality rendering page exceeds native addressability")
    })?;
    let mut page = Vec::with_capacity(expected);
    for source in &mut sources {
        if source.separator_before && source.logical_start - 1 >= page_start {
            page.push(b'\n');
        }
        let source_end = source
            .logical_start
            .checked_add(source.artifact.size())
            .ok_or_else(|| render_failure("quality rendering source range overflowed"))?;
        if source_end <= page_start {
            continue;
        }
        let logical_read_start = page_start.max(source.logical_start);
        let artifact_offset = logical_read_start - source.logical_start;
        let count = usize::try_from(source_end - logical_read_start).map_err(|_| {
            render_failure("quality rendering artifact page exceeds native addressability")
        })?;
        let chunk = source
            .reader
            .read_chunk_at(artifact_offset, count)
            .map_err(|error| failure::artifact(&error))?
            .ok_or_else(|| render_failure("quality output artifact ended before its render page"))?;
        if chunk.offset() != artifact_offset || chunk.bytes().len() != count {
            return Err(render_failure(
                "quality output artifact returned an inexact render page",
            ));
        }
        page.extend_from_slice(chunk.bytes());
    }
    if page.len() != expected {
        return Err(render_failure(
            "quality rendering page differs from terminal artifact accounting",
        ));
    }
    Ok(page)
}

fn render_failure(detail: &'static str) -> DispatchFailure {
    adapter_failure("quality-render-source", detail)
}

pub fn text(value: impl Into<String>) -> BoundedText {
    let mut value = value.into().replace('\0', "\\0");
    if value.is_empty() {
        value.push_str("(no detail)");
    }
    truncate_utf8(&mut value, BoundedText::MAX_BYTES);
    BoundedText::new(value).expect("sanitized nonempty bounded quality text")
}
