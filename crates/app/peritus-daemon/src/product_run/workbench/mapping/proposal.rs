//! Exact explicit proposal acceptance retains model authorship and immutable source bytes.

use super::{ControlError, ControlIntent, ControlOperation, ControlStore, ControlText, Error};
use peritus_app_protocol::{WorkbenchCommand, WorkbenchIntent};
use peritus_product_runner::{
    attachment::ValidatedFileText,
    control::{ConversationId, FileObservation, FileVersion, OperationId},
};
use peritus_types::{ActorId, ArtifactId};

pub(super) fn resolve(
    store: &ControlStore,
    actor: ActorId,
    command: &WorkbenchCommand,
) -> Result<Option<ControlIntent>, Error> {
    let WorkbenchIntent::AcceptBriefProposal { field, proposal, digest } = command.intent() else {
        return Ok(None);
    };
    let id = ConversationId::new(command.query().conversation().into_bytes())?;
    let record = store.load(id)?.ok_or(ControlError::NotFound)?;
    if record.owner_bytes() != actor.as_bytes()
        || record.workspace_bytes() != command.query().workspace().as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    let reference = record
        .replies()
        .iter()
        .find(|reply| reply.operation().as_bytes() == proposal.as_bytes())
        .ok_or(ControlError::NotFound)?;
    if reference.digest() != *digest {
        return Err(ControlError::StaleRevision.into());
    }
    let field = super::brief::domain_field(*field);
    if reference.bytes() <= 8192 {
        return Ok(Some(ControlIntent::SetBrief {
            field,
            text: ControlText::new(store.reply_text(reference)?)?,
        }));
    }
    let observation =
        FileObservation::new(*digest, reference.bytes(), (0, reference.bytes()), *digest)?;
    let version = FileVersion::new(
        OperationId::new(command.operation().into_bytes())?,
        ArtifactId::new(command.operation().into_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        observation,
        peritus_codec::sha256(&consent(actor, command)?),
    )?;
    Ok(Some(ControlIntent::AcceptBriefProposal { field, reply: reference.clone(), version }))
}

pub(in crate::product_run::workbench) fn accept(
    store: &mut ControlStore,
    operation: &ControlOperation,
    actor: ActorId,
    command: &WorkbenchCommand,
) -> Result<peritus_product_runner::control::ControlReceipt, Error> {
    let ControlIntent::AcceptBriefProposal { reply, .. } = operation.intent() else {
        return store.accept(operation);
    };
    // Resolve is performed before this read by the caller. The metadata-only mapping also
    // permits reconnect receipt lookup without copying or reparsing the full proposal.
    let text = store.reply_text(reply)?;
    if text.trim().is_empty()
        || text.chars().any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t'))
    {
        return Err(ControlError::InvalidInput.into());
    }
    let text = ValidatedFileText::new(text.into_bytes())?;
    store.accept_file(operation, &text, consent(actor, command)?)
}

fn consent(actor: ActorId, command: &WorkbenchCommand) -> Result<Vec<u8>, Error> {
    let WorkbenchIntent::AcceptBriefProposal { field, proposal, digest } = command.intent() else {
        return Err(ControlError::InvalidInput.into());
    };
    // An explicit versioned fixed-field representation makes the stored proof independent
    // of transport framing and retains every authority and identity boundary.
    let mut bytes = b"peritus-accepted-brief-proposal-v1\0".to_vec();
    bytes.extend_from_slice(actor.as_bytes());
    bytes.extend_from_slice(command.query().workspace().as_bytes());
    bytes.extend_from_slice(command.query().conversation().as_bytes());
    bytes.extend_from_slice(command.operation().as_bytes());
    bytes.extend_from_slice(&command.expected_revision().to_be_bytes());
    bytes.push(match field {
        peritus_app_protocol::WorkbenchBriefField::Objective => 0,
        peritus_app_protocol::WorkbenchBriefField::Acceptance => 1,
        peritus_app_protocol::WorkbenchBriefField::Constraints => 2,
        peritus_app_protocol::WorkbenchBriefField::Assumptions => 3,
    });
    bytes.extend_from_slice(proposal.as_bytes());
    bytes.extend_from_slice(digest.as_bytes());
    Ok(bytes)
}
