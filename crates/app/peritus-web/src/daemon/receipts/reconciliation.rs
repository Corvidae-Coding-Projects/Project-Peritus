//! Read-only proof of native effects from their retained request and baseline.
use super::{App, AppMessage, AppRequestPayload, AppResponsePayload, NativeOwner, Result, Value, STANDARD, json, problem, response, retain_reconciled_response};
use peritus_app_protocol::{
    AppErrorCode, ProductInteractionQuery, ProductRunControlAction, ProductRunPhase,
    ProductRunReferencePage, ProductRunReferenceQuery, ProductRunSnapshot,
};
use base64::Engine as _;

/// Returns a retained exact response, or proves the requested postcondition from a fresh durable
/// run observation and the pre-request baseline. It never retransmits an uncertain mutation.
pub async fn observed(app: &App, operation: &str) -> Result<Option<Value>> {
    use peritus_app_protocol::{AppProtocolLimits, decode_app_message};
    let Some(record) = app.operation(&format!("daemon:{operation}"))?
    else {
        return Ok(None);
    };
    let frame = STANDARD
        .decode(record.input["frame"].as_str().ok_or_else(|| problem("Missing daemon request"))?)
        .map_err(problem)?;
    let AppMessage::Request(request) =
        decode_app_message(&frame, AppProtocolLimits::PRODUCTION).map_err(problem)?
    else {
        return Err(problem("Invalid retained daemon request"));
    };
    let retained_owner = super::owner::retained(&record.input, &request)?;
    if let Some(value) = record.result {
        let frame = STANDARD
            .decode(value["frame"].as_str().ok_or_else(|| problem("Missing daemon receipt"))?)
            .map_err(problem)?;
        let AppMessage::Response(envelope) =
            decode_app_message(&frame, AppProtocolLimits::PRODUCTION).map_err(problem)?
        else {
            return Err(problem("Invalid retained daemon receipt"));
        };
        if envelope.context() != request.context()
            || envelope.request_id() != request.request_id()
            || envelope.correlation_id() != request.correlation_id()
        {
            return Err(problem("The retained response belongs to another native request"));
        }
        if let AppResponsePayload::Error(error) = envelope.payload() {
            return Ok(Some(json!({"error":format!("Daemon rejected the request: {error}")})));
        }
        let projected = match retained_owner.as_ref() {
            Some(owner) => {
                super::super::response_owned(app, owner, envelope.payload().clone()).await?
            }
            None => response(envelope.payload().clone())?,
        };
        return Ok(Some(projected));
    }
    let Some(owner) = retained_owner else { return Ok(None) };
    let Some(payload) = reconcile(app, &owner, request.payload(), &record.input["baseline"]).await? else {
        return Ok(None);
    };
    let payload = retain_reconciled_response(app, &format!("daemon:{operation}"), &request, payload).await?;
    if let AppResponsePayload::Error(error) = &payload {
        return Ok(Some(json!({"error":format!("Daemon rejected the request: {error}")})));
    }
    let mut recovered = super::super::response_owned(app, &owner, payload).await?;
    if let Some(object) = recovered.as_object_mut() {
        object.insert("recovered".to_owned(), Value::Bool(true));
    }
    Ok(Some(recovered))
}

pub(super) async fn recovery_baseline(app: &App, owner: &NativeOwner, payload: &AppRequestPayload) -> Result<Value> {
    let run_id = match payload {
        AppRequestPayload::ControlProductRun(control) => control.run_id(),
        _ => return Ok(Value::Null),
    };
    match query_run_reference(app, owner, run_id).await? {
        Some((_, snapshot)) => Ok(json!({"run":super::super::snapshot(&snapshot)})),
        None => Ok(json!({"missing":true})),
    }
}

const fn run_id(payload: &AppRequestPayload) -> Option<peritus_types::RunId> {
    match payload {
        AppRequestPayload::UpdateModels(update) => Some(update.run_id()),
        AppRequestPayload::ControlProductRun(control) => Some(control.run_id()),
        _ => None,
    }
}

async fn reconcile(
    app: &App,
    owner: &NativeOwner,
    request: &AppRequestPayload,
    baseline: &Value,
) -> Result<Option<AppResponsePayload>> {
    let Some(run_id) = run_id(request) else { return Ok(None) };
    match request {
        AppRequestPayload::UpdateModels(update) => {
            let current = super::super::raw_request_owned(
                app,
                owner,
                AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run_id)),
            )
            .await?;
            let AppResponsePayload::Interaction(snapshot) = current else { return Ok(None) };
            if snapshot.models() == update.models() {
                Ok(Some(AppResponsePayload::Interaction(snapshot)))
            } else {
                Ok(None)
            }
        }
        AppRequestPayload::ControlProductRun(control) => {
            let Some((page, snapshot)) = query_run_reference(app, owner, run_id).await? else {
                return Ok(None);
            };
            if control_postcondition(control.action(), &snapshot, baseline) {
                Ok(Some(AppResponsePayload::ProductRunReferencePage(page)))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

async fn query_run_reference(
    app: &App,
    owner: &NativeOwner,
    run_id: peritus_types::RunId,
) -> Result<Option<(ProductRunReferencePage, ProductRunSnapshot)>> {
    let payload = super::super::raw_request_owned(
        app,
        owner,
        AppRequestPayload::QueryProductRunReferences(ProductRunReferenceQuery::exact(run_id)),
    )
    .await?;
    let page = match payload {
        AppResponsePayload::ProductRunReferencePage(page) => page,
        AppResponsePayload::Error(error) if error.code() == AppErrorCode::InvalidIdentifier => {
            return Ok(None);
        }
        _ => return Ok(None),
    };
    if page.entries().len() != 1
        || page.next().is_some()
        || page.entries()[0].snapshot().run_id() != run_id
    {
        return Err(problem("The daemon returned a different product run during reconciliation"));
    }
    let snapshot = super::super::hydrate_run(app, owner.target(), page.entries()[0].snapshot())
        .await?;
    Ok(Some((page, snapshot)))
}

fn control_postcondition(
    action: ProductRunControlAction,
    snapshot: &peritus_app_protocol::ProductRunSnapshot,
    baseline: &Value,
) -> bool {
    let phase = snapshot.phase();
    let deliverable = snapshot.deliverable();
    match action {
        ProductRunControlAction::Cancel => phase == ProductRunPhase::Cancelled,
        ProductRunControlAction::Retry => {
            matches!(
                baseline["run"]["phase"].as_str(),
                Some("Failed" | "Cancelled" | "RecoveryRequired")
            ) && baseline["run"]["phase"] != format!("{phase:?}")
        }
        ProductRunControlAction::Export => {
            deliverable.is_some_and(|value| !value.export_path().is_empty())
        }
        ProductRunControlAction::Discard => {
            deliverable.is_some_and(peritus_app_protocol::ProductDeliverable::discarded)
        }
        ProductRunControlAction::Acknowledge => {
            baseline["run"]["operation"]["legalControls"]["acknowledge"] == true
                && baseline["run"]["operation"]["identity"] != snapshot.operation().identity()
        }
        ProductRunControlAction::Accept | ProductRunControlAction::Commit => false,
    }
}
