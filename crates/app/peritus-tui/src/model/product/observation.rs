//! Polling and daemon-observation updates for the product-run screen.

use peritus_app_protocol::{
    AppRequestPayload, ProductInteractionQuery, ProductInteractionSnapshot, ProductRunQuery,
    ProductRunSettlementSnapshot, ProductRunSnapshot,
};
use peritus_types::RunId;

use super::ProductUi;
use crate::{
    action::Effect,
    model::{AppModel, PendingRequest},
};

impl AppModel {
    pub(in crate::model) fn accept_observation_query(
        &mut self,
        observations: &[peritus_app_protocol::ProductRunObservation],
        exact: Option<RunId>,
    ) {
        let snapshots =
            observations.iter().map(|value| value.snapshot().clone()).collect::<Vec<_>>();
        self.accept_product_query(&snapshots, exact);
        if let Some(product) = &mut self.product {
            for value in observations
                .iter()
                .filter(|value| exact.is_none_or(|id| id == value.snapshot().run_id()))
            {
                let run = value.snapshot().run_id();
                if let Some(settlement) = value.settlement() {
                    product.settlements.insert(run, settlement);
                } else {
                    product.settlements.remove(&run);
                }
            }
        }
    }

    pub(in crate::model) fn accept_product_query(
        &mut self,
        snapshots: &[ProductRunSnapshot],
        exact: Option<RunId>,
    ) {
        if let Some(run_id) = exact {
            if let Some(snapshot) = snapshots.iter().find(|value| value.run_id() == run_id) {
                self.accept_product_run(snapshot.clone());
            }
        } else {
            self.accept_product_runs(snapshots.to_vec());
        }
    }

    pub(in crate::model) fn poll_product_runs(&mut self) -> Vec<Effect> {
        if self.product.is_none() || self.context.is_none() {
            return Vec::new();
        }
        let mut effects: Vec<Effect> = self.poll_chat();
        if self.view == crate::model::View::Preview
            && let Some(page) = self.product.as_ref().and_then(|product| product.preview.as_ref())
            && page
                .launches()
                .iter()
                .any(|launch| launch.state() == peritus_app_protocol::WorkbenchLaunchState::Running)
        {
            effects.extend(self.refresh_preview(page.query()));
        }
        if self.pending.values().any(|pending| {
            matches!(pending, PendingRequest::ProductQuery | PendingRequest::ProductExactQuery(_))
        }) {
            return effects;
        }
        effects.extend(self.request(
            AppRequestPayload::QueryProductRunObservations(ProductRunQuery::recent()),
            PendingRequest::ProductQuery,
        ));
        if let Some(run_id) =
            self.product.as_ref().and_then(ProductUi::selected_run).map(ProductRunSnapshot::run_id)
            && let Some(effect) = self.request(
                AppRequestPayload::QueryProductRunObservations(ProductRunQuery::exact(run_id)),
                PendingRequest::ProductExactQuery(run_id),
            )
        {
            effects.push(effect);
        }
        if !self
            .pending
            .values()
            .any(|pending| {
                matches!(
                    pending,
                    PendingRequest::ProductInteractionQuery
                        | PendingRequest::ProductActivityPage(_)
                )
            })
            && let Some(run_id) = self
                .product
                .as_ref()
                .and_then(ProductUi::selected_run)
                .map(ProductRunSnapshot::run_id)
        {
            if self.supports_activity_pages() {
                effects.extend(self.request_activity_history(run_id));
            } else if let Some(effect) = self.request(
                AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run_id)),
                PendingRequest::ProductInteractionQuery,
            ) {
                effects.push(effect);
            }
        }
        effects
    }

    pub(in crate::model) fn accept_product_runs(&mut self, snapshots: Vec<ProductRunSnapshot>) {
        if let Some(product) = &mut self.product {
            let selected = product.selected_run().map(ProductRunSnapshot::run_id);
            product.runs = snapshots;
            product.selected = selected
                .and_then(|id| product.runs.iter().position(|run| run.run_id() == id))
                .unwrap_or_else(|| product.selected.min(product.runs.len().saturating_sub(1)));
            product
                .settlements
                .retain(|run_id, _| product.runs.iter().any(|run| run.run_id() == *run_id));
        }
    }

    pub(in crate::model) fn accept_product_run(&mut self, snapshot: ProductRunSnapshot) {
        let Some(product) = &mut self.product else { return };
        if let Some(existing) =
            product.runs.iter_mut().find(|run| run.run_id() == snapshot.run_id())
        {
            *existing = snapshot;
        } else {
            product.runs.insert(0, snapshot);
            product.selected = 0;
        }
    }

    pub(in crate::model) fn accept_product_settlement(
        &mut self,
        settled: &ProductRunSettlementSnapshot,
    ) {
        let run_id = settled.snapshot().run_id();
        self.accept_product_run(settled.snapshot().clone());
        if let Some(product) = &mut self.product {
            product.settlements.insert(run_id, *settled.settlement());
            product.confirmation = None;
        }
    }

    pub(in crate::model) fn accept_product_interaction(
        &mut self,
        conversation: ProductInteractionSnapshot,
    ) {
        let Some(product) = &mut self.product else { return };
        if product
            .selected_run()
            .is_some_and(|run| run.run_id() == conversation.snapshot().run_id())
        {
            product.conversation = Some(conversation);
        }
    }
}
