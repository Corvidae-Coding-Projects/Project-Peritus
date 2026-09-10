//! Canonical bounded launch, capture, result and artifact-feedback codecs.

mod profile;
#[cfg(test)]
mod tests;

pub(super) use profile::{read_profile, write_profile};

use super::primitive::{invalid, read_digest, read_id, write_digest, write_id};
use crate::{
    ControlOperationId, MAX_WORKBENCH_ARTIFACT_FEEDBACK, MAX_WORKBENCH_CAPTURES,
    MAX_WORKBENCH_INTERACTIONS, MAX_WORKBENCH_LAUNCHES, WorkbenchArtifactFeedback,
    WorkbenchArtifactRegion, WorkbenchCaptureCapability, WorkbenchCaptureConsent,
    WorkbenchCaptureReceipt, WorkbenchCaptureRequest, WorkbenchCaptureState,
    WorkbenchCaptureTarget, WorkbenchInteractionReceipt, WorkbenchLaunchResult,
    WorkbenchLaunchState, WorkbenchLaunchText, WorkbenchResultPage, WorkbenchResultQuery,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{ArtifactId, ProcessId, RunId};

pub(super) fn write_query(
    w: &mut CanonicalWriter,
    value: WorkbenchResultQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    write_id(w, value.run().as_bytes())
}

pub(super) fn read_query(r: &mut CanonicalReader<'_>) -> Result<WorkbenchResultQuery, CodecError> {
    Ok(WorkbenchResultQuery::new(super::workbench::read_query(r)?, read_id(r, RunId::new)?))
}

pub(super) fn write_capture_request(
    w: &mut CanonicalWriter,
    value: WorkbenchCaptureRequest,
) -> Result<(), CodecError> {
    write_id(w, value.launch().as_bytes())?;
    write_target(w, value.target())?;
    w.write_u16(value.consent().tag())
}

pub(super) fn read_capture_request(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCaptureRequest, CodecError> {
    let launch = read_id(r, ControlOperationId::new)?;
    let target = read_target(r)?;
    let offset = r.offset();
    let consent = WorkbenchCaptureConsent::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    Ok(WorkbenchCaptureRequest::new(launch, target, consent))
}

fn write_target(w: &mut CanonicalWriter, value: WorkbenchCaptureTarget) -> Result<(), CodecError> {
    match value {
        WorkbenchCaptureTarget::X11Window(window) => {
            w.write_u16(1)?;
            w.write_u64(window)
        }
    }
}

fn read_target(r: &mut CanonicalReader<'_>) -> Result<WorkbenchCaptureTarget, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        1 => invalid(offset, WorkbenchCaptureTarget::x11_window(r.read_u64()?)),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

pub(super) fn write_region(
    w: &mut CanonicalWriter,
    value: Option<WorkbenchArtifactRegion>,
) -> Result<(), CodecError> {
    w.write_bool(value.is_some())?;
    if let Some(region) = value {
        let (x, y, width, height) = region.coordinates();
        w.write_u32(x)?;
        w.write_u32(y)?;
        w.write_u32(width)?;
        w.write_u32(height)?;
    }
    Ok(())
}

pub(super) fn read_region(
    r: &mut CanonicalReader<'_>,
) -> Result<Option<WorkbenchArtifactRegion>, CodecError> {
    if !r.read_bool()? {
        return Ok(None);
    }
    let offset = r.offset();
    invalid(
        offset,
        WorkbenchArtifactRegion::new(r.read_u32()?, r.read_u32()?, r.read_u32()?, r.read_u32()?),
    )
    .map(Some)
}

pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchResultPage,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    w.write_u64(value.control_revision())?;
    w.write_u64(value.result_revision())?;
    match value.capability() {
        WorkbenchCaptureCapability::X11SelectedWindow => w.write_u16(1)?,
        WorkbenchCaptureCapability::Unavailable(reason) => {
            w.write_u16(2)?;
            w.write_str(reason.as_str())?;
        }
    }
    write_count(w, value.launches().len())?;
    for launch in value.launches() {
        write_launch(w, launch)?;
    }
    Ok(())
}

pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<WorkbenchResultPage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let control_revision = r.read_u64()?;
    let result_revision = r.read_u64()?;
    let capability_offset = r.offset();
    let capability = match r.read_u16()? {
        1 => WorkbenchCaptureCapability::X11SelectedWindow,
        2 => WorkbenchCaptureCapability::Unavailable(read_text(r)?),
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, capability_offset)),
    };
    let count = read_count(r, MAX_WORKBENCH_LAUNCHES)?;
    let mut launches = Vec::with_capacity(count);
    for _ in 0..count {
        launches.push(read_launch(r)?);
    }
    invalid(
        offset,
        WorkbenchResultPage::new(query, control_revision, result_revision, capability, launches),
    )
}

fn write_launch(w: &mut CanonicalWriter, value: &WorkbenchLaunchResult) -> Result<(), CodecError> {
    write_id(w, value.launch().as_bytes())?;
    write_profile(w, value.profile())?;
    w.write_bool(value.process().is_some())?;
    if let Some(process) = value.process() {
        write_id(w, process.as_bytes())?;
    }
    w.write_u16(value.state().tag())?;
    w.write_bool(value.ready())?;
    write_count(w, value.interactions().len())?;
    for interaction in value.interactions() {
        write_id(w, interaction.operation().as_bytes())?;
        write_digest(w, interaction.digest())?;
        w.write_bool(interaction.observed())?;
    }
    write_count(w, value.captures().len())?;
    for capture in value.captures() {
        write_capture(w, capture)?;
    }
    write_count(w, value.feedback().len())?;
    for feedback in value.feedback() {
        write_feedback(w, feedback)?;
    }
    w.write_u32(value.behavior_checks())?;
    write_optional_digest(w, value.stdout_digest())?;
    w.write_bool(value.exit_code().is_some())?;
    if let Some(code) = value.exit_code() {
        w.write_u32(code)?;
    }
    Ok(())
}

fn read_launch(r: &mut CanonicalReader<'_>) -> Result<WorkbenchLaunchResult, CodecError> {
    let offset = r.offset();
    let launch = read_id(r, ControlOperationId::new)?;
    let profile = read_profile(r)?;
    let process = if r.read_bool()? { Some(read_id(r, ProcessId::new)?) } else { None };
    let state_offset = r.offset();
    let state = WorkbenchLaunchState::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, state_offset))?;
    let ready = r.read_bool()?;
    let interaction_count = read_count(r, MAX_WORKBENCH_INTERACTIONS)?;
    let mut interactions = Vec::with_capacity(interaction_count);
    for _ in 0..interaction_count {
        interactions.push(WorkbenchInteractionReceipt::new(
            read_id(r, ControlOperationId::new)?,
            read_digest(r)?,
            r.read_bool()?,
        ));
    }
    let capture_count = read_count(r, MAX_WORKBENCH_CAPTURES)?;
    let mut captures = Vec::with_capacity(capture_count);
    for _ in 0..capture_count {
        captures.push(read_capture(r)?);
    }
    let feedback_count = read_count(r, MAX_WORKBENCH_ARTIFACT_FEEDBACK)?;
    let mut feedback = Vec::with_capacity(feedback_count);
    for _ in 0..feedback_count {
        feedback.push(read_feedback(r)?);
    }
    let behavior_checks = r.read_u32()?;
    let stdout_digest = read_optional_digest(r)?;
    let exit_code = if r.read_bool()? { Some(r.read_u32()?) } else { None };
    invalid(
        offset,
        WorkbenchLaunchResult::new(
            launch,
            profile,
            process,
            state,
            ready,
            interactions,
            captures,
            feedback,
            behavior_checks,
            stdout_digest,
            exit_code,
        ),
    )
}

fn write_capture(
    w: &mut CanonicalWriter,
    value: &WorkbenchCaptureReceipt,
) -> Result<(), CodecError> {
    write_id(w, value.operation().as_bytes())?;
    w.write_u16(value.state().tag())?;
    write_target(w, value.target())?;
    let complete = value.artifact().is_some();
    w.write_bool(complete)?;
    if let (Some(artifact), Some(digest), Some((width, height)), Some(captured)) =
        (value.artifact(), value.image_digest(), value.dimensions(), value.captured_unix_millis())
    {
        write_id(w, artifact.as_bytes())?;
        write_digest(w, digest)?;
        w.write_u32(width)?;
        w.write_u32(height)?;
        w.write_u64(captured)?;
    }
    w.write_str(value.detail().as_str())
}

fn read_capture(r: &mut CanonicalReader<'_>) -> Result<WorkbenchCaptureReceipt, CodecError> {
    let offset = r.offset();
    let operation = read_id(r, ControlOperationId::new)?;
    let state = WorkbenchCaptureState::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    let target = read_target(r)?;
    let (artifact, digest, dimensions, captured) = if r.read_bool()? {
        (
            Some(read_id(r, ArtifactId::new)?),
            Some(read_digest(r)?),
            Some((r.read_u32()?, r.read_u32()?)),
            Some(r.read_u64()?),
        )
    } else {
        (None, None, None, None)
    };
    let detail = read_text(r)?;
    invalid(
        offset,
        WorkbenchCaptureReceipt::new(
            operation,
            operation_state(state),
            target,
            artifact,
            digest,
            dimensions,
            captured,
            detail,
        ),
    )
}

const fn operation_state(state: WorkbenchCaptureState) -> WorkbenchCaptureState {
    state
}

fn write_feedback(
    w: &mut CanonicalWriter,
    value: &WorkbenchArtifactFeedback,
) -> Result<(), CodecError> {
    write_id(w, value.operation().as_bytes())?;
    write_id(w, value.capture().as_bytes())?;
    w.write_u16(value.feedback().tag())?;
    w.write_str(value.message().as_str())?;
    write_region(w, value.region())
}

fn read_feedback(r: &mut CanonicalReader<'_>) -> Result<WorkbenchArtifactFeedback, CodecError> {
    Ok(WorkbenchArtifactFeedback::new(
        read_id(r, ControlOperationId::new)?,
        read_id(r, ControlOperationId::new)?,
        super::workbench_review::read_feedback(r)?,
        super::workbench_inputs::read_text(r)?,
        read_region(r)?,
    ))
}

fn write_optional_digest(
    w: &mut CanonicalWriter,
    value: Option<peritus_types::Sha256Digest>,
) -> Result<(), CodecError> {
    w.write_bool(value.is_some())?;
    if let Some(value) = value {
        write_digest(w, value)?;
    }
    Ok(())
}

fn read_optional_digest(
    r: &mut CanonicalReader<'_>,
) -> Result<Option<peritus_types::Sha256Digest>, CodecError> {
    if r.read_bool()? { read_digest(r).map(Some) } else { Ok(None) }
}

fn read_text(r: &mut CanonicalReader<'_>) -> Result<WorkbenchLaunchText, CodecError> {
    let offset = r.offset();
    invalid(offset, WorkbenchLaunchText::new(r.read_str()?.to_owned()))
}

fn write_count(w: &mut CanonicalWriter, count: usize) -> Result<(), CodecError> {
    w.write_u16(
        u16::try_from(count).map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )
}

fn read_count(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let offset = r.offset();
    let count = usize::from(r.read_u16()?);
    if count > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(count)
}
