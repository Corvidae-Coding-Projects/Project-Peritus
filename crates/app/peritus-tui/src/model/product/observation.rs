//! Polling and daemon-observation updates for the product-run screen.

use peritus_app_protocol::{
    AppRequestPayload, ProductRunConversation, ProductRunConversationQuery, ProductRunQuery,
    ProductRunSettlementSnapshot, ProductRunSnapshot,
};

use super::ProductUi;
use crate::{
    action::Effect,
    model::{AppModel, PendingRequest},
};

impl AppModel {
    pub(in crate::model) fn poll_product_runs(&mut self) -> Vec<Effect> {
        if self.product.is_none()
            || self.context.is_none()
            || self.pending.values().any(|pending| matches!(pending, PendingRequest::ProductQuery))
        {
            return Vec::new();
        }
        let mut effects: Vec<Effect> = self
            .request(
                AppRequestPayload::QueryProductRuns(ProductRunQuery::recent()),
                PendingRequest::ProductQuery,
            )
            .into_iter()
            .collect();
        if let Some(run_id) =
            self.product.as_ref().and_then(ProductUi::selected_run).map(ProductRunSnapshot::run_id)
            && let Some(effect) = self.request(
                AppRequestPayload::QueryProductRuns(ProductRunQuery::exact(run_id)),
                PendingRequest::ProductQuery,
            )
        {
            effects.push(effect);
        }
        if !self
            .pending
            .values()
            .any(|pending| matches!(pending, PendingRequest::ProductConversationQuery))
            && let Some(run_id) = self
                .product
                .as_ref()
                .and_then(ProductUi::selected_run)
                .map(ProductRunSnapshot::run_id)
            && let Some(effect) = self.request(
                AppRequestPayload::QueryProductRunConversation(ProductRunConversationQuery::new(
                    run_id,
                )),
                PendingRequest::ProductConversationQuery,
            )
        {
            effects.push(effect);
        }
        effects
    }

    pub(in crate::model) fn accept_product_runs(&mut self, snapshots: Vec<ProductRunSnapshot>) {
        if let Some(product) = &mut self.product {
            product.runs = snapshots;
            product.selected = product.selected.min(product.runs.len().saturating_sub(1));
            product
                .settlements
                .retain(|run_id, _| product.runs.iter().any(|run| run.run_id() == *run_id));
        }
    }

    pub(in crate::model) fn accept_product_settlements(
        &mut self,
        settled: &[ProductRunSettlementSnapshot],
    ) {
        let snapshots = settled
            .iter()
            .map(|value| (value.snapshot().clone(), value.snapshot().run_id(), *value.settlement()))
            .collect::<Vec<_>>();
        self.accept_product_runs(snapshots.iter().map(|value| value.0.clone()).collect());
        if let Some(product) = &mut self.product {
            for (_, run_id, settlement) in snapshots {
                product.settlements.insert(run_id, settlement);
            }
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

    pub(in crate::model) fn accept_product_conversation(
        &mut self,
        conversation: ProductRunConversation,
    ) {
        let Some(product) = &mut self.product else { return };
        if product.selected_run().is_some_and(|run| run.run_id() == conversation.run_id()) {
            product.conversation = Some(conversation);
        }
    }
}
