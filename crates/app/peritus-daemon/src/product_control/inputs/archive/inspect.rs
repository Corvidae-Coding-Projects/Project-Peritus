//! Content-free projection of a verified manifest; never decodes private model content.

use super::{Error, Manifest};
use peritus_app_protocol::{
    WorkbenchContextDisposition as D, WorkbenchContextRow, WorkbenchContextSeal,
    WorkbenchContextSource as S, WorkbenchInputId, WorkbenchInputSelection, WorkbenchInvocationId,
    WorkbenchMessageRole,
};
use peritus_product_runner::control::ControlError;
use peritus_types::Sha256Digest;

type SealedProjection = (WorkbenchContextSeal, u64, Vec<WorkbenchContextRow>);

pub(in crate::product_control) fn inspect_manifest(
    bytes: &[u8],
) -> Result<SealedProjection, Error> {
    if bytes.len() > 1024 * 1024 {
        return Err(ControlError::Capacity.into());
    }
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|_| Error::Corrupt("invalid context manifest"))?;
    let id = invocation(*manifest.invocation.as_bytes())?;
    let seal = WorkbenchContextSeal::new(
        id,
        Sha256Digest::new(manifest.request_digest),
        peritus_codec::sha256(bytes),
        manifest.generation,
    );
    let mut rows = Vec::new();
    for source in manifest.sources {
        let selected = WorkbenchInputSelection::new(
            WorkbenchInputId::new(*source.selection.id().as_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            source.selection.revision(),
        )
        .map_err(|_| ControlError::InvalidInput)?;
        rows.push(row(S::Input(selected), source.digest, source.bytes)?);
    }
    for reply in manifest.public_replies {
        rows.push(row(
            S::PublicReply(invocation(*reply.after_invocation().as_bytes())?),
            reply.digest().into_bytes(),
            reply.bytes(),
        )?);
    }
    for image in manifest.images {
        rows.push(row(
            crate::product_control::context::image_source(&image)?,
            image.digest().into_bytes(),
            image.bytes(),
        )?);
    }
    for file in manifest.files {
        rows.push(row(
            crate::product_control::context::file_source(
                file.attachment.operation(),
                file.version.operation(),
            )?,
            file.version.observation().digest().into_bytes(),
            file.version.observation().bytes(),
        )?);
    }
    for (index, message) in manifest.messages.into_iter().enumerate() {
        use super::super::manifest::MessageRole as R;
        let role = match message.role {
            R::System => WorkbenchMessageRole::System,
            R::Developer => WorkbenchMessageRole::Developer,
            R::User => WorkbenchMessageRole::User,
            R::Assistant => WorkbenchMessageRole::Assistant,
            R::Tool => WorkbenchMessageRole::Tool,
        };
        let ordinal = u32::try_from(index).map_err(|_| ControlError::Capacity)?;
        rows.push(row(S::Message { ordinal, role }, message.digest, message.encoded_bytes)?);
    }
    Ok((seal, manifest.request_bytes, rows))
}

fn invocation(bytes: [u8; 16]) -> Result<WorkbenchInvocationId, Error> {
    WorkbenchInvocationId::new(bytes).map_err(|_| ControlError::InvalidInput.into())
}
fn row(source: S, digest: [u8; 32], bytes: u64) -> Result<WorkbenchContextRow, Error> {
    WorkbenchContextRow::new(source, Sha256Digest::new(digest), bytes, D::Included)
        .map_err(|_| Error::Corrupt("invalid context source metadata"))
}
