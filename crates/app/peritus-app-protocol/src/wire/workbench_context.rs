//! Exact bounded content-free context wire forms.

use super::primitive::{invalid, read_id, unknown, write_id};
use crate::{
    MAX_WORKBENCH_CONTEXT_PAGE, WorkbenchContextDisposition as D, WorkbenchContextPage,
    WorkbenchContextPreference as P, WorkbenchContextQuery, WorkbenchContextRow,
    WorkbenchContextSeal, WorkbenchContextSource as S, WorkbenchContextView as V, WorkbenchInputId,
    WorkbenchInputSelection, WorkbenchInvocationId, WorkbenchMessageRole as R,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::Sha256Digest;

#[cfg(test)]
mod tests;

pub(super) fn write_query(
    w: &mut CanonicalWriter,
    value: WorkbenchContextQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    w.write_u32(value.offset())?;
    match value.view() {
        V::Next => w.write_u16(1),
        V::History => w.write_u16(2),
        V::Invocation(id) => {
            w.write_u16(3)?;
            write_id(w, id.as_bytes())
        }
    }
}
pub(super) fn read_query(r: &mut CanonicalReader<'_>) -> Result<WorkbenchContextQuery, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let start = r.read_u32()?;
    let view = match r.read_u16()? {
        1 => V::Next,
        2 => V::History,
        3 => V::Invocation(read_id(r, WorkbenchInvocationId::new)?),
        _ => return unknown(offset),
    };
    invalid(offset, WorkbenchContextQuery::new(query, revision, start, view))
}
pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchContextPage,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    w.write_u32(value.total())?;
    w.write_bool(value.seal().is_some())?;
    if let Some(seal) = value.seal() {
        write_id(w, seal.invocation().as_bytes())?;
        w.write_fixed(seal.request_digest().as_bytes())?;
        w.write_fixed(seal.manifest_digest().as_bytes())?;
        w.write_u64(seal.generation())?;
    }
    w.write_u16(
        u16::try_from(value.rows().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for row in value.rows() {
        write_source(w, row.source())?;
        w.write_fixed(row.digest().as_bytes())?;
        w.write_u64(row.bytes())?;
        w.write_u16(match row.disposition() {
            D::Eligible => 1,
            D::Included => 2,
            D::Held => 3,
            D::Withdrawn => 4,
            D::Superseded => 5,
            D::DependencyBlocked => 6,
            D::AwaitingLaterInput => 7,
            D::Deselected => 8,
            D::UserExcluded => 9,
        })?;
        w.write_bool(row.preference().is_some())?;
        if let Some(preference) = row.preference() {
            w.write_u16(match preference {
                P::Pinned => 1,
                P::Excluded => 2,
            })?;
        }
    }
    Ok(())
}
pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<WorkbenchContextPage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let total = r.read_u32()?;
    let seal = if r.read_bool()? {
        Some(WorkbenchContextSeal::new(
            read_id(r, WorkbenchInvocationId::new)?,
            Sha256Digest::new(r.read_fixed()?),
            Sha256Digest::new(r.read_fixed()?),
            r.read_u64()?,
        ))
    } else {
        None
    };
    let count = usize::from(r.read_u16()?);
    if count > MAX_WORKBENCH_CONTEXT_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let source = read_source(r)?;
        let digest = Sha256Digest::new(r.read_fixed()?);
        let bytes = r.read_u64()?;
        let disposition = match r.read_u16()? {
            1 => D::Eligible,
            2 => D::Included,
            3 => D::Held,
            4 => D::Withdrawn,
            5 => D::Superseded,
            6 => D::DependencyBlocked,
            7 => D::AwaitingLaterInput,
            8 => D::Deselected,
            9 => D::UserExcluded,
            _ => return unknown(offset),
        };
        let preference = if r.read_bool()? {
            Some(match r.read_u16()? {
                1 => P::Pinned,
                2 => P::Excluded,
                _ => return unknown(offset),
            })
        } else {
            None
        };
        let row = invalid(offset, WorkbenchContextRow::new(source, digest, bytes, disposition))?;
        rows.push(match preference {
            Some(preference) => invalid(offset, row.with_preference(preference))?,
            None => row,
        });
    }
    invalid(offset, WorkbenchContextPage::new(query, total, seal, rows))
}
pub(super) fn write_source(w: &mut CanonicalWriter, value: S) -> Result<(), CodecError> {
    match value {
        S::File { attachment, version } => {
            w.write_u16(6)?;
            write_id(w, attachment.as_bytes())?;
            write_id(w, version.as_bytes())
        }
        S::Image { operation, input, artifact } => {
            w.write_u16(5)?;
            write_id(w, operation.as_bytes())?;
            write_id(w, input.as_bytes())?;
            write_id(w, artifact.as_bytes())
        }
        S::Input(selected) => {
            w.write_u16(1)?;
            write_id(w, selected.id().as_bytes())?;
            w.write_u64(selected.revision())
        }
        S::PublicReply(id) => {
            w.write_u16(2)?;
            write_id(w, id.as_bytes())
        }
        S::Message { ordinal, role } => {
            w.write_u16(3)?;
            w.write_u32(ordinal)?;
            w.write_u16(match role {
                R::System => 1,
                R::Developer => 2,
                R::User => 3,
                R::Assistant => 4,
                R::Tool => 5,
            })
        }
        S::Invocation { id, manifest_digest } => {
            w.write_u16(4)?;
            write_id(w, id.as_bytes())?;
            w.write_fixed(manifest_digest.as_bytes())
        }
    }
}
pub(super) fn read_source(r: &mut CanonicalReader<'_>) -> Result<S, CodecError> {
    let offset = r.offset();
    Ok(match r.read_u16()? {
        1 => S::Input(invalid(
            offset,
            WorkbenchInputSelection::new(read_id(r, WorkbenchInputId::new)?, r.read_u64()?),
        )?),
        2 => S::PublicReply(read_id(r, WorkbenchInvocationId::new)?),
        3 => {
            let ordinal = r.read_u32()?;
            let role = match r.read_u16()? {
                1 => R::System,
                2 => R::Developer,
                3 => R::User,
                4 => R::Assistant,
                5 => R::Tool,
                _ => return unknown(offset),
            };
            S::Message { ordinal, role }
        }
        4 => S::Invocation {
            id: read_id(r, WorkbenchInvocationId::new)?,
            manifest_digest: Sha256Digest::new(r.read_fixed()?),
        },
        6 => S::File {
            attachment: read_id(r, crate::ControlOperationId::new)?,
            version: read_id(r, crate::ControlOperationId::new)?,
        },
        5 => S::Image {
            operation: read_id(r, crate::ControlOperationId::new)?,
            input: read_id(r, WorkbenchInputId::new)?,
            artifact: read_id(r, peritus_types::ArtifactId::new)?,
        },
        _ => return unknown(offset),
    })
}
