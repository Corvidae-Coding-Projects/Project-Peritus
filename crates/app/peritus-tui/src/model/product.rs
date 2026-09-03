//! Interactive product-run composer and daemon-observation projection.

mod interaction;
mod observation;

use std::collections::BTreeMap;
use std::path::PathBuf;

use peritus_app_protocol::{
    AppRequestPayload, ProductProviderSelection, ProductRunContinuation, ProductRunControl,
    ProductRunControlAction, ProductRunConversation, ProductRunRequest, ProductRunSnapshot,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateStage, EvidenceStatus, QualificationEvidence, RunSettlement,
};
use peritus_types::RunId;

use super::{AppModel, Editor, EditorKind, Effect, NoticeLevel, PendingRequest};
use crate::runtime::ProductLaunchContext;

#[derive(Debug)]
pub struct ProductUi {
    pub launch: ProductLaunchContext,
    pub runs: Vec<ProductRunSnapshot>,
    pub selected: usize,
    pub conversation: Option<ProductRunConversation>,
    pub settlements: BTreeMap<RunId, RunSettlement>,
    pub confirmation: Option<CandidateConfirmation>,
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
            conversation: None,
            settlements: BTreeMap::new(),
            confirmation: None,
            writer: default,
            reviewer: default,
            fixer: default,
        }
    }

    pub fn selected_run(&self) -> Option<&ProductRunSnapshot> {
        self.runs.get(self.selected)
    }
    pub fn selected_conversation(&self) -> Option<&ProductRunConversation> {
        let selected = self.selected_run()?.run_id();
        self.conversation.as_ref().filter(|conversation| conversation.run_id() == selected)
    }
    pub fn selected_settlement(&self) -> Option<&RunSettlement> {
        self.settlements.get(&self.selected_run()?.run_id())
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateConfirmation {
    pub run_id: RunId,
    pub action: ProductRunControlAction,
    pub warning: String,
}

#[derive(Clone, Copy)]
pub(super) enum ProviderRole {
    Writer,
    Reviewer,
    Fixer,
}

impl AppModel {
    pub(super) fn run_selected_product_candidate(&mut self) -> Vec<Effect> {
        let Some((workspace, instruction, candidate_digest)) =
            self.product.as_ref().and_then(|product| {
                let run = product.selected_run().filter(|run| run.phase().terminal())?;
                let deliverable = run.deliverable()?;
                let checkpoint = product.selected_settlement()?.checkpoint()?;
                Some((
                    PathBuf::from(deliverable.workspace_path()),
                    deliverable.run_instructions().to_owned(),
                    checkpoint.identity().candidate_digest(),
                ))
            })
        else {
            self.notice(
                NoticeLevel::Warning,
                "run is available after the coding run stops with an exact candidate identity",
            );
            return Vec::new();
        };
        vec![Effect::RunCandidate { workspace, instruction, candidate_digest }]
    }

    pub(crate) fn candidate_run_finished(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => self.notice(NoticeLevel::Info, "candidate command completed successfully"),
            Err(error) => self.notice(NoticeLevel::Error, error),
        }
    }

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

    pub(super) fn open_product_message_composer(&mut self) {
        let Some(run_id) =
            self.product.as_ref().and_then(ProductUi::selected_run).map(ProductRunSnapshot::run_id)
        else {
            self.notice(NoticeLevel::Warning, "select a coding run before sending a message");
            return;
        };
        self.editor = Some(Editor {
            kind: EditorKind::ProductMessage(run_id),
            title: "Message this coding run",
            hint: "Reply, redirect, add context, or say continue. Shift-Enter adds a line.",
            buffer: String::new(),
            cursor: 0,
        });
    }

    pub(super) fn submit_product_message(&mut self, run_id: RunId, message: String) -> Vec<Effect> {
        let continuation = match ProductRunContinuation::new(run_id, message) {
            Ok(continuation) => continuation,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        self.request(
            AppRequestPayload::ContinueProductRun(continuation),
            PendingRequest::ProductContinue,
        )
        .into_iter()
        .collect()
    }

    pub(super) fn control_selected_product_run(
        &mut self,
        action: ProductRunControlAction,
    ) -> Vec<Effect> {
        let Some((run_id, phase, qualification, has_deliverable)) =
            self.product.as_ref().and_then(ProductUi::selected_run).map(|run| {
                (
                    run.run_id(),
                    run.phase(),
                    run.deliverable().map(peritus_app_protocol::ProductDeliverable::qualification),
                    run.deliverable().is_some(),
                )
            })
        else {
            self.notice(NoticeLevel::Warning, "no coding run is selected");
            return Vec::new();
        };
        if matches!(action, ProductRunControlAction::Cancel)
            && phase.terminal()
            && phase != peritus_app_protocol::ProductRunPhase::WaitingForUser
        {
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
        let deliverable_action = matches!(
            action,
            ProductRunControlAction::Accept
                | ProductRunControlAction::Commit
                | ProductRunControlAction::Export
                | ProductRunControlAction::Discard
        );
        if deliverable_action && (!phase.terminal() || !has_deliverable) {
            self.notice(
                NoticeLevel::Warning,
                "deliverable actions are available after the run stops with a candidate",
            );
            return Vec::new();
        }
        if matches!(action, ProductRunControlAction::Accept | ProductRunControlAction::Commit)
            && has_deliverable
            && qualification != Some(CandidateStage::Qualified)
        {
            let warning = self.unqualified_warning(run_id, action);
            let confirmed = self
                .product
                .as_ref()
                .and_then(|product| product.confirmation.as_ref())
                .is_some_and(|pending| pending.run_id == run_id && pending.action == action);
            if !confirmed {
                if let Some(product) = &mut self.product {
                    product.confirmation =
                        Some(CandidateConfirmation { run_id, action, warning: warning.clone() });
                }
                self.notice(NoticeLevel::Warning, warning);
                return Vec::new();
            }
        }
        if let Some(product) = &mut self.product {
            product.confirmation = None;
        }
        self.request(
            AppRequestPayload::ControlProductRun(ProductRunControl::new(run_id, action)),
            PendingRequest::ProductControl,
        )
        .into_iter()
        .collect()
    }

    fn unqualified_warning(&self, run_id: RunId, action: ProductRunControlAction) -> String {
        let action = match action {
            ProductRunControlAction::Accept => "accept",
            ProductRunControlAction::Commit => "commit",
            _ => "use",
        };
        let evidence = self
            .product
            .as_ref()
            .and_then(|product| product.settlements.get(&run_id))
            .and_then(RunSettlement::checkpoint)
            .map_or_else(|| "qualification evidence is incomplete".to_owned(), missing_evidence);
        format!(
            "Unqualified candidate: {evidence}. Press the {action} key again to confirm {action}."
        )
    }

    pub(super) fn cycle_product_provider(&mut self, role: ProviderRole) {
        let Some(product) = &mut self.product else { return };
        product.cycle_role(role);
        self.notice(NoticeLevel::Info, "provider role selection updated for the next run");
    }

    pub(super) fn select_previous_product(&mut self) -> bool {
        let Some(product) = &mut self.product else { return false };
        product.selected = product.selected.saturating_sub(1);
        product.conversation = None;
        product.confirmation = None;
        true
    }

    pub(super) fn select_next_product(&mut self) -> bool {
        let Some(product) = &mut self.product else { return false };
        product.selected = (product.selected + 1).min(product.runs.len().saturating_sub(1));
        product.conversation = None;
        product.confirmation = None;
        true
    }
}

fn missing_evidence(checkpoint: &CandidateCheckpoint) -> String {
    let mut missing = Vec::new();
    append_evidence("deterministic checks", checkpoint.gates(), &mut missing);
    append_evidence("public requirements", checkpoint.obligations(), &mut missing);
    append_evidence("independent review", checkpoint.review(), &mut missing);
    if missing.is_empty() { "qualification is incomplete".to_owned() } else { missing.join(", ") }
}

fn append_evidence(
    name: &str,
    evidence: &EvidenceStatus<QualificationEvidence>,
    missing: &mut Vec<String>,
) {
    let state = match evidence {
        EvidenceStatus::Missing => Some("missing"),
        EvidenceStatus::Failed(_) => Some("failed"),
        EvidenceStatus::Stale(_) => Some("stale"),
        EvidenceStatus::Current(record) if !record.value().satisfied() => Some("failed"),
        EvidenceStatus::Current(_) => None,
    };
    if let Some(state) = state {
        missing.push(format!("{name} {state}"));
    }
}
