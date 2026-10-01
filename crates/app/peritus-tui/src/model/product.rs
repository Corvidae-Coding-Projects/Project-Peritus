//! Interactive product-run composer and daemon-observation projection.

mod composer;
mod interaction;
mod observation;
mod preview;
mod review;

pub use review::{ReviewDraft, ReviewFocus};

use std::collections::BTreeMap;
use std::path::PathBuf;

use peritus_app_protocol::{
    AppRequestPayload, ProductProviderSelection, ProductRunControl, ProductRunControlAction,
    ProductRunConversation, ProductRunSnapshot,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, QualificationEvidence,
    RunSettlement,
};
use peritus_types::RunId;

use super::{AppModel, Effect, NoticeLevel, PendingRequest};
use crate::runtime::ProductLaunchContext;

#[derive(Debug)]
pub struct ProductUi {
    pub launch: ProductLaunchContext,
    pub runs: Vec<ProductRunSnapshot>,
    pub selected: usize,
    pub conversation: Option<ProductRunConversation>,
    pub settlements: BTreeMap<RunId, RunSettlement>,
    pub confirmation: Option<CandidateConfirmation>,
    pub detail_scroll: u16,
    pub inspection_scroll: u16,
    pub preview: Option<peritus_app_protocol::WorkbenchResultPage>,
    pub preview_scroll: u16,
    pub preview_outputs: Vec<peritus_app_protocol::WorkbenchPreviewOutput>,
    pub preview_query: Option<peritus_app_protocol::WorkbenchResultQuery>,
    pub preview_message: String,
    pub(crate) review: review::DiffReviewUi,
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
            detail_scroll: 0,
            inspection_scroll: 0,
            preview: None,
            preview_scroll: 0,
            preview_outputs: Vec::new(),
            preview_query: None,
            preview_message: String::new(),
            review: review::DiffReviewUi::default(),
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

    pub(in crate::model) fn providers(&self) -> Option<ProductProviderSelection> {
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
    snapshot: ProductRunSnapshot,
    identity: Option<CandidateIdentity>,
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
                let deliverable =
                    run.deliverable().filter(|deliverable| !deliverable.discarded())?;
                let checkpoint = product.selected_settlement()?.checkpoint()?;
                Some((
                    PathBuf::from(deliverable.workspace_path()),
                    deliverable.run_instructions().to_owned(),
                    checkpoint.identity().repository_digest(),
                ))
            })
        else {
            self.notice(
                NoticeLevel::Warning,
                "run is available after the coding run stops with a current, undiscarded candidate",
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

    pub(super) fn control_selected_product_run(
        &mut self,
        action: ProductRunControlAction,
    ) -> Vec<Effect> {
        let Some(snapshot) = self.product.as_ref().and_then(ProductUi::selected_run).cloned()
        else {
            self.notice(NoticeLevel::Warning, "no coding run is selected");
            return Vec::new();
        };
        let run_id = snapshot.run_id();
        let phase = snapshot.phase();
        let qualification =
            snapshot.deliverable().map(peritus_app_protocol::ProductDeliverable::qualification);
        let has_deliverable = snapshot.deliverable().is_some();
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
        if let Some((level, message)) = unavailable_handoff(&snapshot, action) {
            self.notice(level, message);
            return Vec::new();
        }
        if matches!(action, ProductRunControlAction::Accept | ProductRunControlAction::Commit)
            && has_deliverable
            && qualification != Some(CandidateStage::Qualified)
        {
            let warning = self.unqualified_warning(run_id, action);
            let identity = self
                .product
                .as_ref()
                .and_then(ProductUi::selected_settlement)
                .and_then(RunSettlement::checkpoint)
                .map(|checkpoint| *checkpoint.identity());
            let confirmed = self
                .product
                .as_ref()
                .and_then(|product| product.confirmation.as_ref())
                .is_some_and(|pending| {
                    pending.run_id == run_id
                        && pending.action == action
                        && pending.snapshot == snapshot
                        && pending.identity == identity
                });
            if !confirmed {
                if let Some(product) = &mut self.product {
                    product.confirmation = Some(CandidateConfirmation {
                        run_id,
                        action,
                        warning: warning.clone(),
                        snapshot,
                        identity,
                    });
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
        product.detail_scroll = 0;
        product.review.clear();
        true
    }

    pub(super) fn select_next_product(&mut self) -> bool {
        let Some(product) = &mut self.product else { return false };
        product.selected = (product.selected + 1).min(product.runs.len().saturating_sub(1));
        product.conversation = None;
        product.confirmation = None;
        product.detail_scroll = 0;
        product.review.clear();
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

fn unavailable_handoff(
    snapshot: &ProductRunSnapshot,
    action: ProductRunControlAction,
) -> Option<(NoticeLevel, &'static str)> {
    let deliverable_action = matches!(
        action,
        ProductRunControlAction::Accept
            | ProductRunControlAction::Commit
            | ProductRunControlAction::Export
            | ProductRunControlAction::Discard
    );
    if deliverable_action && (!snapshot.phase().terminal() || snapshot.deliverable().is_none()) {
        return Some((
            NoticeLevel::Warning,
            "deliverable actions are available after the run stops with a candidate",
        ));
    }
    let deliverable = snapshot.deliverable()?;
    if deliverable_action
        && deliverable.discarded()
        && (action != ProductRunControlAction::Export || deliverable.export_path().is_empty())
    {
        return Some((
            NoticeLevel::Info,
            "This candidate was discarded. Continue the conversation to create a new candidate.",
        ));
    }
    if action == ProductRunControlAction::Discard && !deliverable.commit_revision().is_empty() {
        return Some((
            NoticeLevel::Info,
            "This candidate is already committed. Use Git to revert the commit.",
        ));
    }
    None
}
