//! Additive local diagnostics codecs with preallocation limits.

use super::primitive::{invalid, read_id, unknown, write_id};
use crate::{DoctorFinding, DoctorQuery, DoctorReport, DoctorStatus, MAX_DOCTOR_FINDINGS};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{ProviderProfileId, WorkspaceId};

pub(super) fn write_query(w: &mut CanonicalWriter, value: DoctorQuery) -> Result<(), CodecError> {
    write_id(w, value.workspace().as_bytes())?;
    w.write_option_tag(value.provider().is_some())?;
    if let Some(provider) = value.provider() {
        write_id(w, provider.as_bytes())?;
    }
    Ok(())
}

pub(super) fn read_query(r: &mut CanonicalReader<'_>) -> Result<DoctorQuery, CodecError> {
    let workspace = read_id(r, WorkspaceId::new)?;
    let provider =
        if r.read_option_tag()? { Some(read_id(r, ProviderProfileId::new)?) } else { None };
    Ok(DoctorQuery::new(workspace, provider))
}

pub(super) fn write_report(
    w: &mut CanonicalWriter,
    value: &DoctorReport,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    w.write_u16(
        u16::try_from(value.findings().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for finding in value.findings() {
        w.write_str(finding.check())?;
        w.write_u16(match finding.status() {
            DoctorStatus::Healthy => 1,
            DoctorStatus::Warning => 2,
            DoctorStatus::Blocked => 3,
            DoctorStatus::Unsupported => 4,
        })?;
        w.write_str(finding.observation())?;
        w.write_str(finding.action())?;
    }
    Ok(())
}

pub(super) fn read_report(r: &mut CanonicalReader<'_>) -> Result<DoctorReport, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let count = usize::from(r.read_u16()?);
    if count > MAX_DOCTOR_FINDINGS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut findings = Vec::with_capacity(count);
    for _ in 0..count {
        let check = read_text(r, 64)?;
        let offset = r.offset();
        let status = match r.read_u16()? {
            1 => DoctorStatus::Healthy,
            2 => DoctorStatus::Warning,
            3 => DoctorStatus::Blocked,
            4 => DoctorStatus::Unsupported,
            _ => return unknown(offset),
        };
        let observation = read_text(r, crate::MAX_DOCTOR_TEXT_BYTES)?;
        let action = read_text(r, crate::MAX_DOCTOR_TEXT_BYTES)?;
        findings.push(invalid(offset, DoctorFinding::new(check, status, observation, action))?);
    }
    invalid(offset, DoctorReport::new(query, findings))
}

fn read_text(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<String, CodecError> {
    let offset = r.offset();
    let text = r.read_str()?;
    if text.len() > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(text.to_owned())
}
