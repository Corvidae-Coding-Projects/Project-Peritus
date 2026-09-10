//! Exact additive doctor request/report compatibility frames.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, DoctorFinding, DoctorQuery,
    DoctorReport, DoctorStatus,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query = DoctorQuery::new(id(32, peritus_types::WorkspaceId::new), None);
    let report = DoctorReport::new(
        query,
        vec![
            DoctorFinding::new(
                "provider-authentication".to_owned(),
                DoctorStatus::Unsupported,
                "No authentication probe was run.".to_owned(),
                "Use explicit provider setup if needed.".to_owned(),
            )
            .expect("finding"),
        ],
    )
    .expect("report");
    Ok(vec![
        encoded(
            "minimal-doctor-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::Doctor(query)),
            limits,
        )?,
        encoded(
            "realistic-doctor-report",
            FixtureClass::Realistic,
            &AppResponseEnvelope::new(
                context(),
                id(10, crate::RequestId::new),
                id(11, crate::CorrelationId::new),
                AppResponsePayload::Doctor(report),
            ),
            limits,
        )?,
    ])
}
