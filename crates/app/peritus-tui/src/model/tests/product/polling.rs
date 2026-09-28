use super::*;
use crate::model::PendingRequest;
use peritus_app_protocol::{AppResponseEnvelope, AppResponsePayload, ProductRunQuery};

#[test]
fn slow_dashboard_request_does_not_starve_conversation_polling() {
    let (mut model, run, _) = unqualified_model();
    model.chat.run_id = Some(run);
    assert!(model.pending.values().any(|pending| matches!(pending, PendingRequest::ProductQuery)));
    let effects = model.poll_product_runs();
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::Send(AppMessage::Request(request)) if matches!(request.payload(),
            AppRequestPayload::QueryInteraction(query) if query.run_id() == run))));
    assert!(
        !effects.iter().any(|effect| matches!(effect,
        Effect::Send(AppMessage::Request(request)) if matches!(request.payload(),
            AppRequestPayload::QueryProductRuns(_)))),
        "no duplicate dashboard query"
    );
}

#[test]
fn expired_exact_reply_does_not_become_a_recent_runs_page() {
    let (mut model, run, _) = unqualified_model();
    let snapshot = model.product.as_ref().unwrap().runs[0].clone();
    let effect = model
        .request(
            AppRequestPayload::QueryProductRuns(ProductRunQuery::exact(run)),
            PendingRequest::ProductExactQuery(run),
        )
        .unwrap();
    let Effect::Send(AppMessage::Request(request)) = effect else { panic!("request") };
    model.pending_started.retain(|id, _| *id == request.request_id());
    model.tick_count = 120;
    assert!(!model.expire_pending_requests());
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::ProductRuns(Vec::new()),
    ))));
    assert_eq!(model.product.as_ref().unwrap().runs, vec![snapshot]);
}
