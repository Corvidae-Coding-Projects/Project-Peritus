//! Exact A3 guidance conversion into independent G4 audit and C0 sidecar mutations.

use crate::product_control::{ControlStoreError as Error, guidance::GuidanceMutation};
use peritus_app_protocol::{
    WorkbenchGuidanceContent as AppContent, WorkbenchGuidanceScope as AppScope,
    WorkbenchGuidanceSelection as AppSelection, WorkbenchGuidanceSource as AppSource,
    WorkbenchIntent,
};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlText, GuidanceAudit, GuidanceContent, GuidanceScope,
    GuidanceSelection, GuidanceSource, InvocationId, OperationId,
};

pub(super) const fn is_guidance(intent: &WorkbenchIntent) -> bool {
    matches!(
        intent,
        WorkbenchIntent::SaveGuidance(_)
            | WorkbenchIntent::ReviseGuidance(_)
            | WorkbenchIntent::PinGuidance(_)
            | WorkbenchIntent::ScopeGuidance(_)
            | WorkbenchIntent::ForgetGuidance(_)
    )
}

pub(super) fn mutation(intent: &WorkbenchIntent) -> Result<GuidanceMutation, Error> {
    match intent {
        WorkbenchIntent::SaveGuidance(value) => Ok(GuidanceMutation::Save(value.clone())),
        WorkbenchIntent::ReviseGuidance(value) => Ok(GuidanceMutation::Revise(value.clone())),
        WorkbenchIntent::PinGuidance(value) => Ok(GuidanceMutation::Pin(*value)),
        WorkbenchIntent::ScopeGuidance(value) => Ok(GuidanceMutation::Scope(*value)),
        WorkbenchIntent::ForgetGuidance(value) => Ok(GuidanceMutation::Forget(value.clone())),
        _ => Err(ControlError::InvalidInput.into()),
    }
}

pub(super) fn domain_intent(intent: &WorkbenchIntent) -> Result<ControlIntent, Error> {
    let audit = match intent {
        WorkbenchIntent::SaveGuidance(value) => GuidanceAudit::save(
            value.expected_dependency_revision(),
            domain_content(value.content())?,
            value.pinned(),
        ),
        WorkbenchIntent::ReviseGuidance(value) => GuidanceAudit::revise(
            domain_selection(value.selection())?,
            value.expected_dependency_revision(),
            domain_content(value.content())?,
        )?,
        WorkbenchIntent::PinGuidance(value) => GuidanceAudit::pin(
            domain_selection(value.selection())?,
            value.expected_dependency_revision(),
            value.pinned(),
        )?,
        WorkbenchIntent::ScopeGuidance(value) => GuidanceAudit::scope(
            domain_selection(value.selection())?,
            value.expected_dependency_revision(),
            domain_scope(value.scope())?,
        )?,
        WorkbenchIntent::ForgetGuidance(value) => GuidanceAudit::forget(
            domain_selection(value.selection())?,
            value.expected_dependency_revision(),
            ControlText::new(value.reason().as_str().to_owned())?,
        )?,
        _ => return Err(ControlError::InvalidInput.into()),
    };
    Ok(ControlIntent::UpdateGuidance(audit))
}

fn domain_selection(value: AppSelection) -> Result<GuidanceSelection, Error> {
    Ok(GuidanceSelection::new(
        OperationId::new(value.id().into_bytes())?,
        value.expected_revision(),
    )?)
}

fn domain_content(value: &AppContent) -> Result<GuidanceContent, Error> {
    let source = match value.source() {
        AppSource::UserAuthored => GuidanceSource::UserAuthored,
        AppSource::AcceptedPublicReply { operation, invocation, digest } => {
            GuidanceSource::AcceptedPublicReply {
                operation: OperationId::new(operation.into_bytes())?,
                invocation: InvocationId::new(invocation.into_bytes())?,
                digest: digest.into_bytes(),
            }
        }
    };
    Ok(GuidanceContent::new(
        ControlText::new(value.text().as_str().to_owned())?,
        source,
        domain_scope(value.scope())?,
    )?)
}

fn domain_scope(value: AppScope) -> Result<GuidanceScope, Error> {
    Ok(match value {
        AppScope::Project => GuidanceScope::Project,
        AppScope::Conversation(id) => GuidanceScope::Conversation(
            peritus_product_runner::control::ConversationId::new(id.into_bytes())?,
        ),
    })
}
