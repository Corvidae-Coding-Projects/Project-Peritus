//! Additive artifact-backed initialization frames; legacy layouts remain unchanged.
use super::primitive::{invalid, read_digest, unknown, write_digest};
use crate::{
    InitArtifactDiscovery, InitArtifactPage, InitArtifactPageRequest, InitArtifactProposal,
    InitContentReference, InitSourceKind, InitSourceSelection,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};

fn write_content(w: &mut CanonicalWriter, value: InitContentReference) -> Result<(), CodecError> {
    write_digest(w, value.digest())?;
    w.write_u64(value.bytes())
}
fn read_content(r: &mut CanonicalReader<'_>) -> Result<InitContentReference, CodecError> {
    Ok(InitContentReference::new(read_digest(r)?, r.read_u64()?))
}
pub(super) fn write_discovery(
    w: &mut CanonicalWriter,
    value: &InitArtifactDiscovery,
) -> Result<(), CodecError> {
    super::workbench_init::write_discovery_request(w, value.request())?;
    w.write_bool(value.previous().is_some())?;
    if let Some(previous) = value.previous() {
        write_content(w, previous)?;
    }
    w.write_bool(value.source().is_some())?;
    if let Some(source) = value.source() {
        w.write_str(source.path())?;
        w.write_u16(match source.kind() {
            InitSourceKind::Manifest => 1,
            InitSourceKind::Documentation => 2,
            InitSourceKind::Instructions => 3,
            InitSourceKind::CommandConfig => 4,
        })?;
    }
    w.write_bool(value.command().is_some())?;
    if let Some(command) = value.command() {
        w.write_u64(command)?;
    }
    Ok(())
}
pub(super) fn read_discovery(
    r: &mut CanonicalReader<'_>,
) -> Result<InitArtifactDiscovery, CodecError> {
    let offset = r.offset();
    let request = super::workbench_init::read_discovery_request(r)?;
    let previous = if r.read_bool()? { Some(read_content(r)?) } else { None };
    let source = if r.read_bool()? {
        let path = r.read_str()?.to_owned();
        let kind = match r.read_u16()? {
            1 => InitSourceKind::Manifest,
            2 => InitSourceKind::Documentation,
            3 => InitSourceKind::Instructions,
            4 => InitSourceKind::CommandConfig,
            _ => return unknown(offset),
        };
        Some(invalid(offset, InitSourceSelection::new(path, kind))?)
    } else {
        None
    };
    let command = if r.read_bool()? { Some(r.read_u64()?) } else { None };
    invalid(offset, InitArtifactDiscovery::new(request, previous, source, command))
}
pub(in crate::wire) fn write_proposal(
    w: &mut CanonicalWriter,
    value: InitArtifactProposal,
) -> Result<(), CodecError> {
    super::workbench_init::write_discovery_request(w, value.request())?;
    write_content(w, value.manifest())?;
    write_content(w, value.review())
}
pub(in crate::wire) fn read_proposal(
    r: &mut CanonicalReader<'_>,
) -> Result<InitArtifactProposal, CodecError> {
    Ok(InitArtifactProposal::new(
        super::workbench_init::read_discovery_request(r)?,
        read_content(r)?,
        read_content(r)?,
    ))
}
pub(super) fn write_page_request(
    w: &mut CanonicalWriter,
    value: InitArtifactPageRequest,
) -> Result<(), CodecError> {
    write_proposal(w, value.proposal())?;
    w.write_u64(value.offset())?;
    w.write_u32(value.maximum())
}
pub(super) fn read_page_request(
    r: &mut CanonicalReader<'_>,
) -> Result<InitArtifactPageRequest, CodecError> {
    let offset = r.offset();
    invalid(offset, InitArtifactPageRequest::new(read_proposal(r)?, r.read_u64()?, r.read_u32()?))
}
pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &InitArtifactPage,
) -> Result<(), CodecError> {
    write_page_request(w, value.request())?;
    w.write_bytes(value.bytes())
}
pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<InitArtifactPage, CodecError> {
    let offset = r.offset();
    invalid(offset, InitArtifactPage::new(read_page_request(r)?, r.read_bytes()?.to_vec()))
}
