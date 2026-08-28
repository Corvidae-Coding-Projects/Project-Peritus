//! Interactive product-run composer and daemon-observation projection.

use peritus_app_protocol::{
    AppRequestPayload, ProductProviderSelection, ProductRunControl, ProductRunControlAction,
    ProductRunQuery, ProductRunRequest, ProductRunSnapshot,
};

use super::{AppModel, Editor, EditorKind, Effect, NoticeLevel, PendingRequest};
use crate::runtime::ProductLaunchContext;

#[derive(Debug)]
pub struct ProductUi {
    pub launch: ProductLaunchContext,
    pub runs: Vec<ProductRunSnapshot>,
    pub selected: usize,
    writer: usize,
    reviewer: usize,
    fixer: usize,
}

impl ProductUi {
    pub(super) fn new(launch: ProductLaunchContext) -> Self {
        let default = launch.default_provider().unwrap_or(0);
        Self {
            launch,
            runs: Vec::new(),
            selected: 0,
            writer: default,
            reviewer: default,
            fixer: default,
        }
    }

    pub fn selected_run(&self) -> Option<&ProductRunSnapshot> {
        self.runs.get(self.selected)
    }
    pub fn writer_label(&self) -> &str {
        self.launch.providers().get(self.writer).map_or("No provider", |provider| provider.label())
    }
    pub fn reviewer_label(&self) -> &str {
        self.launch
            .providers()
            .get(self.reviewer)
            .map_or("No provider", |provider| provider.label())
    }
    pub fn fixer_label(&self) -> &str {
        self.launch.providers().get(self.fixer).map_or("No provider", |provider| provider.label())
    }

    fn providers(&self) -> Option<ProductProviderSelection> {
        Some(ProductProviderSelection::new(
            self.launch.providers().get(self.writer)?.profile_id(),
            self.launch.providers().get(self.reviewer)?.profile_id(),
            self.launch.providers().get(self.fixer)?.profile_id(),
        ))
    }

    fn cycle_role(&mut self, role: ProviderRole) {
        let count = self.launch.providers().len();
        if count == 0 {
            return;
        }
        match role {
            ProviderRole::Writer => self.writer = (self.writer + 1) % count,
            ProviderRole::Reviewer => self.reviewer = (self.reviewer + 1) % count,
            ProviderRole::Fixer => self.fixer = (self.fixer + 1) % count,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ProviderRole {
    Writer,
    Reviewer,
    Fixer,
}

impl AppModel {
    pub(super) fn open_task_composer(&mut self) {
        if self.product.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Start Peritus through the `peritus` command to create coding runs",
            );
            return;
        }
        self.editor = Some(Editor {
            kind: EditorKind::ProductTask,
            title: "New coding task",
            hint: "Describe the outcome. Shift-Enter adds a line; Enter starts the run.",
            buffer: String::new(),
            cursor: 0,
        });
    }

    pub(super) fn submit_product_task(&mut self, task: String) -> Vec<Effect> {
        let Some(run_id) = self.ids.run() else {
            self.notice(NoticeLevel::Error, "could not allocate a run identity");
            return Vec::new();
        };
        let Some(product) = &self.product else { return Vec::new() };
        let Some(providers) = product.providers() else {
            self.notice(
                NoticeLevel::Warning,
                "No provider is configured. Run `peritus providers` to sign in or add one.",
            );
            return Vec::new();
        };
        let request =
            match ProductRunRequest::new(run_id, product.launch.workspace_id(), providers, task) {
                Ok(request) => request,
                Err(error) => {
                    self.notice(NoticeLevel::Error, error.to_string());
                    return Vec::new();
                }
            };
        self.request(AppRequestPayload::StartProductRun(request), PendingRequest::ProductStart)
            .into_iter()
            .collect()
    }

    pub(super) fn poll_product_runs(&mut self) -> Vec<Effect> {
        if self.product.is_none()
            || self.context.is_none()
            || self.pending.values().any(|pending| matches!(pending, PendingRequest::ProductQuery))
        {
            return Vec::new();
        }
        self.request(
            AppRequestPayload::QueryProductRuns(ProductRunQuery::recent()),
            PendingRequest::ProductQuery,
        )
        .into_iter()
        .collect()
    }

    pub(super) fn accept_product_runs(&mut self, snapshots: Vec<ProductRunSnapshot>) {
        if let Some(product) = &mut self.product {
            product.runs = snapshots;
            product.selected = product.selected.min(product.runs.len().saturating_sub(1));
        }
    }

    pub(super) fn accept_product_run(&mut self, snapshot: ProductRunSnapshot) {
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

    pub(super) fn control_selected_product_run(
        &mut self,
        action: ProductRunControlAction,
    ) -> Vec<Effect> {
        let Some((run_id, phase)) = self
            .product
            .as_ref()
            .and_then(ProductUi::selected_run)
            .map(|run| (run.run_id(), run.phase()))
        else {
            self.notice(NoticeLevel::Warning, "no coding run is selected");
            return Vec::new();
        };
        if matches!(action, ProductRunControlAction::Cancel) && phase.terminal() {
            self.notice(NoticeLevel::Warning, "the selected coding run is already finished");
            return Vec::new();
        }
        if matches!(action, ProductRunControlAction::Retry) && !phase.retryable() {
            self.notice(
                NoticeLevel::Warning,
                "retry is available only for failed, cancelled, or interrupted runs",
            );
            return Vec::new();
        }
        self.request(
            AppRequestPayload::ControlProductRun(ProductRunControl::new(run_id, action)),
            PendingRequest::ProductControl,
        )
        .into_iter()
        .collect()
    }

    pub(super) fn cycle_product_provider(&mut self, role: ProviderRole) {
        let Some(product) = &mut self.product else { return };
        product.cycle_role(role);
        self.notice(NoticeLevel::Info, "provider role selection updated for the next run");
    }

    pub(super) const fn select_previous_product(&mut self) -> bool {
        let Some(product) = &mut self.product else { return false };
        product.selected = product.selected.saturating_sub(1);
        true
    }

    pub(super) fn select_next_product(&mut self) -> bool {
        let Some(product) = &mut self.product else { return false };
        product.selected = (product.selected + 1).min(product.runs.len().saturating_sub(1));
        true
    }
}
