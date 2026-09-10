//! Permission query, mutation and effective-policy compatibility frames.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchCommand, WorkbenchIntent, WorkbenchPermissionCapability, WorkbenchPermissionChange,
    WorkbenchPermissionEntry, WorkbenchPermissionProvenance, WorkbenchPermissions, WorkbenchQuery,
    WorkbenchWorkspaceTrust,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(121, ConversationId::new), id(122, peritus_types::WorkspaceId::new));
    let change = WorkbenchCommand::new(
        id(123, ControlOperationId::new),
        query,
        7,
        WorkbenchIntent::SetPermissions(WorkbenchPermissionChange::new(
            2,
            WorkbenchPermissionCapability::Network,
            false,
        )),
    );
    let rows = WorkbenchPermissionCapability::ALL.map(|capability| {
        WorkbenchPermissionEntry::new(
            capability,
            true,
            capability != WorkbenchPermissionCapability::Network,
            if capability == WorkbenchPermissionCapability::Network {
                WorkbenchPermissionProvenance::UserRestriction
            } else {
                WorkbenchPermissionProvenance::WorkspaceHostPolicy
            },
            matches!(
                capability,
                WorkbenchPermissionCapability::Write | WorkbenchPermissionCapability::Process
            ),
        )
        .expect("entry")
    });
    let response = AppResponseEnvelope::new(
        context(),
        id(10, crate::RequestId::new),
        id(11, crate::CorrelationId::new),
        AppResponsePayload::WorkbenchPermissions(
            WorkbenchPermissions::new(query, 8, 3, WorkbenchWorkspaceTrust::Managed, rows)
                .expect("permissions"),
        ),
    );
    Ok(vec![
        encoded(
            "minimal-workbench-permissions-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryWorkbenchPermissions(query)),
            limits,
        )?,
        encoded(
            "realistic-workbench-permissions-change",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(change)),
            limits,
        )?,
        encoded("realistic-workbench-permissions", FixtureClass::Realistic, &response, limits)?,
    ])
}
