//! Bounded connection-owned model reads cannot hold up controls, events, or other requests.

use super::{
    AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope, AppResponsePayload,
    ProductRunService, product_run_error,
};
use std::{
    future::Future,
    task::{Context, Poll},
};
use tokio::task::{JoinError, JoinSet};

const MAX_DISCOVERIES: usize = 4;

#[derive(Default)]
pub(in crate::session) struct CatalogRequests(JoinSet<AppResponseEnvelope>);

impl CatalogRequests {
    pub(in crate::session) fn start(
        &mut self,
        service: &ProductRunService,
        actor: peritus_types::ActorId,
        limits: peritus_app_protocol::AppProtocolLimits,
        request: &AppRequestEnvelope,
    ) -> bool {
        let AppRequestPayload::QueryModels(query) = request.payload() else { return false };
        let query = *query;
        let service = service.clone();
        let request = request.clone();
        self.try_spawn(async move {
            let payload = match service.authorize_workbench_request(actor, request.payload()) {
                Err(error) => AppResponsePayload::Error(error),
                Ok(()) => match service.query_models(query).await {
                    Ok(catalog) => AppResponsePayload::Models(catalog),
                    Err(error) => product_run_error(error),
                },
            };
            let payload = super::constrain_error_diagnostic(payload, limits.max_diagnostic_bytes());
            AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                payload,
            )
        })
    }

    pub(in crate::session) fn try_spawn(
        &mut self,
        task: impl Future<Output = AppResponseEnvelope> + Send + 'static,
    ) -> bool {
        if self.0.len() >= MAX_DISCOVERIES {
            return false;
        }
        self.0.spawn(task);
        true
    }

    pub(in crate::session) fn poll(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<Result<AppResponseEnvelope, JoinError>> {
        match self.0.poll_join_next(context) {
            Poll::Ready(Some(result)) => Poll::Ready(result),
            Poll::Ready(None) | Poll::Pending => Poll::Pending,
        }
    }

    pub(in crate::session) async fn shutdown(&mut self) {
        self.0.shutdown().await;
    }
}
