//! Interactive structured-diff selection and revision-fenced comment controls.

use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{
    AppRequestPayload, ControlOperationId, WellKnownProtocolFeature, WorkbenchCommand,
    WorkbenchIntent, WorkbenchReviewCommentState, WorkbenchReviewFeedback, WorkbenchReviewQuery,
};

use super::ProductUi;
use crate::{
    action::Effect,
    model::{AppModel, NoticeLevel, PendingRequest, View},
};

mod diff_pages;
mod editor;
mod state;

pub use editor::ReviewDraft;
pub use state::{DiffReviewUi, ReviewFocus};

impl AppModel {
    pub(in crate::model) fn open_diff_panel(&mut self) -> Vec<Effect> {
        self.view = View::Diff;
        if let Some(product) = &mut self.product {
            product.inspection_scroll = 0;
        }
        self.refresh_review()
    }

    pub(in crate::model) fn refresh_review(&mut self) -> Vec<Effect> {
        let available = self.context.is_some()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchReview.as_str()
            });
        let selected = self.chat.workbench.selected;
        let run = self.product.as_ref().and_then(ProductUi::selected_run);
        let Some((query, run)) = selected.zip(run) else {
            if let Some(product) = &mut self.product {
                product.review.page = None;
                "Raw diff only. Open the governing /sessions conversation to add comments."
                    .clone_into(&mut product.review.message);
            }
            return Vec::new();
        };
        if !available {
            if let Some(product) = &mut self.product {
                product.review.page = None;
                "Raw diff only. Structured workbench review was not negotiated."
                    .clone_into(&mut product.review.message);
            }
            return Vec::new();
        }
        if !run.operation().may_start_execution() {
            if let Some(product) = &mut self.product {
                product.review.page = None;
                "Structured comments open at a terminal effect boundary; raw diff remains available."
                    .clone_into(&mut product.review.message);
            }
            return Vec::new();
        }
        let offset = self
            .product
            .as_ref()
            .and_then(|product| product.review.page.as_ref())
            .map_or(0, |page| page.query().offset());
        self.request_review_page(query, run.run_id(), 0, offset)
    }

    pub(in crate::model) fn accept_review_summary(
        &mut self,
        requested: WorkbenchReviewQuery,
        summary: &peritus_app_protocol::WorkbenchReviewSummary,
    ) -> Vec<Effect> {
        let query = summary.query();
        let pending_matches =
            self.product.as_ref().and_then(|product| product.review.pending).is_some_and(
                |pending| {
                    pending == requested
                        && requested.query() == query.query()
                        && requested.run() == query.run()
                        && requested.offset() == query.offset()
                        && (requested.revision() == 0 || requested.revision() == query.revision())
                },
            );
        if !pending_matches {
            return Vec::new();
        }
        let Ok(page) = peritus_app_protocol::WorkbenchReviewPage::new(
            query,
            summary.candidate_digest(),
            summary.diff_digest(),
            Vec::new(),
            summary.comments().to_vec(),
            summary.total_comments(),
            summary.evidence().to_vec(),
        ) else {
            return Vec::new();
        };
        if summary.structured_available() {
            if let Some(product) = &mut self.product {
                product.review.pending = None;
            }
            return self.accept_review_page(requested, page);
        }
        if self.chat.workbench.selected != Some(query.query())
            || self
                .product
                .as_ref()
                .and_then(ProductUi::selected_run)
                .map(peritus_app_protocol::ProductRunSnapshot::run_id)
                != Some(query.run())
        {
            return Vec::new();
        }
        if let Some(product) = &mut self.product {
            product.review.page = Some(page);
            product.review.pending = None;
            product.review.diff_page = None;
            product.review.pending_diff = None;
            product.review.pending_raw = None;
            product.review.raw_line = None;
            product.review.raw_stream = true;
            product.review.raw_total_bytes = None;
            product.review.raw_lines.clear();
            product.review.raw_index = 0;
            product.review.diff_history.clear();
            product.review.file = 0;
            product.review.hunk = 0;
            product.review.comment = 0;
            product.review.message =
                "Structured diff unavailable; raw diff remains available.".into();
        }
        self.request_raw_stream_location(0)
    }

    fn request_review_page(
        &mut self,
        query: peritus_app_protocol::WorkbenchQuery,
        run: peritus_types::RunId,
        revision: u64,
        offset: u32,
    ) -> Vec<Effect> {
        let request = WorkbenchReviewQuery::new(query, run, revision, offset);
        if self.product.as_ref().and_then(|product| product.review.pending).is_some_and(|pending| {
            pending.query() == query
                && pending.run() == run
                && pending.revision() == revision
                && pending.offset() == offset
        }) {
            return Vec::new();
        }
        let summary = self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::WorkbenchReviewSummary.as_str()
        });
        let (payload, pending) = if summary {
            (
                AppRequestPayload::QueryWorkbenchReviewSummary(request),
                PendingRequest::WorkbenchReviewSummary(request),
            )
        } else {
            (
                AppRequestPayload::QueryWorkbenchReview(request),
                PendingRequest::WorkbenchReview(request),
            )
        };
        let Some(effect) = self.request(payload, pending) else {
            return Vec::new();
        };
        if let Some(product) = &mut self.product {
            product.review.pending = Some(request);
        }
        vec![effect]
    }

    pub(in crate::model) fn review_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        let structured = self.product.as_ref().is_some_and(|product| {
            (product.review.diff_page.is_some() || product.review.page.is_some())
                && !product.review.raw
        });
        match key.code {
            KeyCode::Char('t') => {
                let summary_negotiated = self.features.iter().any(|feature| {
                    feature.as_str() == WellKnownProtocolFeature::WorkbenchReviewSummary.as_str()
                });
                let summary_review_ready = summary_negotiated
                    && self.product.as_ref().is_some_and(|product| {
                        product.review.page.is_some() && product.review.diff_page.is_some()
                    });
                if summary_negotiated
                    && self.product.as_ref().is_some_and(|product| product.review.page.is_some())
                {
                    if !summary_review_ready {
                        return Some(Vec::new());
                    }
                    let leaving_raw_stream =
                        self.product.as_ref().is_some_and(|product| product.review.raw_stream);
                    if let Some(product) = &mut self.product {
                        product.inspection_scroll = 0;
                        product.review.pending_raw = None;
                        product.review.raw_line = None;
                        product.review.raw_total_bytes = None;
                        product.review.raw_index = 0;
                        product.review.raw_stream = !leaving_raw_stream;
                        product.review.raw = !leaving_raw_stream;
                    }
                    return Some(if leaving_raw_stream {
                        Vec::new()
                    } else {
                        self.request_raw_stream_location(0)
                    });
                }
                if let Some(product) = &mut self.product {
                    product.review.raw = !product.review.raw;
                    product.inspection_scroll = 0;
                }
                Some(Vec::new())
            }
            KeyCode::Char('r') => Some(self.refresh_review()),
            KeyCode::Char('n') => Some(self.next_review_diff_page()),
            KeyCode::Char('p') => Some(self.previous_review_diff_page()),
            KeyCode::Char(']') => Some(self.move_raw_cursor(true)),
            KeyCode::Char('[') => Some(self.move_raw_cursor(false)),
            _ if !structured => None,
            KeyCode::Tab => {
                if let Some(product) = &mut self.product {
                    product.review.scroll = 0;
                    product.review.focus = match product.review.focus {
                        ReviewFocus::File => ReviewFocus::Hunk,
                        ReviewFocus::Hunk => ReviewFocus::Comment,
                        ReviewFocus::Comment => ReviewFocus::File,
                    };
                }
                Some(Vec::new())
            }
            KeyCode::Up | KeyCode::Char('k') => Some(self.move_review_selection_page(false)),
            KeyCode::Down | KeyCode::Char('j') => Some(self.move_review_selection_page(true)),
            KeyCode::Left => {
                if let Some(product) = &mut self.product {
                    product.review.scroll = 0;
                    product.review.hunk = product.review.hunk.saturating_sub(1);
                }
                Some(Vec::new())
            }
            KeyCode::Right => {
                if let Some(product) = &mut self.product {
                    product.review.scroll = 0;
                    let maximum = product
                        .review
                        .selected_file()
                        .map_or(0, |file| file.hunks().len().saturating_sub(1));
                    product.review.hunk = (product.review.hunk + 1).min(maximum);
                }
                Some(Vec::new())
            }
            KeyCode::Char('e') => Some(self.open_review_editor(WorkbenchReviewFeedback::Explain)),
            KeyCode::Char('v') => {
                Some(self.open_review_editor(WorkbenchReviewFeedback::RequestRevision))
            }
            KeyCode::Char('K') => {
                Some(self.open_review_editor(WorkbenchReviewFeedback::KeepBehavior))
            }
            KeyCode::Char('l') => {
                Some(self.open_review_editor(WorkbenchReviewFeedback::LeaveAlone))
            }
            KeyCode::Char('b') => Some(self.rebind_selected_comment()),
            KeyCode::Char('d') => Some(self.dismiss_selected_comment()),
            _ => None,
        }
    }

    fn move_review_selection_page(&mut self, forward: bool) -> Vec<Effect> {
        let page = self.product.as_ref().and_then(|product| product.review.page.as_ref());
        if let Some(page) = page
            && self
                .product
                .as_ref()
                .is_some_and(|product| product.review.focus == ReviewFocus::Comment)
        {
            let query = page.query();
            if forward
                && self.product.as_ref().is_some_and(|product| {
                    product.review.comment + 1 >= page.comments().len()
                        && query.offset().saturating_add(
                            u32::try_from(page.comments().len()).unwrap_or(u32::MAX),
                        ) < page.total_comments()
                })
            {
                return self.request_review_page(
                    query.query(),
                    query.run(),
                    query.revision(),
                    query
                        .offset()
                        .saturating_add(u32::try_from(page.comments().len()).unwrap_or(u32::MAX)),
                );
            }
            if !forward
                && self
                    .product
                    .as_ref()
                    .is_some_and(|product| product.review.comment == 0 && query.offset() > 0)
            {
                return self.request_review_page(
                    query.query(),
                    query.run(),
                    query.revision(),
                    query.offset().saturating_sub(
                        u32::try_from(peritus_app_protocol::MAX_WORKBENCH_REVIEW_PAGE)
                            .unwrap_or(u32::MAX),
                    ),
                );
            }
        }
        let Some(product) = &mut self.product else { return Vec::new() };
        let review = &mut product.review;
        review.scroll = 0;
        match review.focus {
            ReviewFocus::File => {
                let maximum =
                    review.page.as_ref().map_or(0, |page| page.files().len().saturating_sub(1));
                review.file = if forward {
                    (review.file + 1).min(maximum)
                } else {
                    review.file.saturating_sub(1)
                };
                review.hunk = 0;
            }
            ReviewFocus::Hunk => {
                let maximum =
                    review.selected_file().map_or(0, |file| file.hunks().len().saturating_sub(1));
                review.hunk = if forward {
                    (review.hunk + 1).min(maximum)
                } else {
                    review.hunk.saturating_sub(1)
                };
            }
            ReviewFocus::Comment => {
                let maximum =
                    review.page.as_ref().map_or(0, |page| page.comments().len().saturating_sub(1));
                review.comment = if forward {
                    (review.comment + 1).min(maximum)
                } else {
                    review.comment.saturating_sub(1)
                };
            }
        }
        Vec::new()
    }

    fn rebind_selected_comment(&mut self) -> Vec<Effect> {
        let Some((query, revision, comment, anchor, stale)) =
            self.product.as_ref().and_then(|product| {
                let page = product.review.page.as_ref()?;
                let comment = product.review.selected_comment()?;
                Some((
                    page.query().query(),
                    page.query().revision(),
                    comment.id(),
                    product.review.selected_anchor()?.clone(),
                    comment.state() == WorkbenchReviewCommentState::Stale,
                ))
            })
        else {
            return Vec::new();
        };
        if !stale {
            self.notice(NoticeLevel::Warning, "Only a stale comment can be explicitly rebound.");
            return Vec::new();
        }
        self.submit_review_intent(
            query,
            revision,
            WorkbenchIntent::RebindReview { comment, anchor },
            String::new(),
        )
    }

    fn dismiss_selected_comment(&mut self) -> Vec<Effect> {
        let Some((query, revision, comment)) = self.product.as_ref().and_then(|product| {
            let page = product.review.page.as_ref()?;
            Some((
                page.query().query(),
                page.query().revision(),
                product.review.selected_comment()?.id(),
            ))
        }) else {
            return Vec::new();
        };
        self.submit_review_intent(
            query,
            revision,
            WorkbenchIntent::DismissReview { comment },
            String::new(),
        )
    }

    fn submit_review_intent(
        &mut self,
        query: peritus_app_protocol::WorkbenchQuery,
        revision: u64,
        intent: WorkbenchIntent,
        draft: String,
    ) -> Vec<Effect> {
        if self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending control receipt before another review edit; draft retained.",
            );
            return Vec::new();
        }
        let Ok(operation) = ControlOperationId::new(self.ids.bytes(b"workbench-review")) else {
            return Vec::new();
        };
        let command = WorkbenchCommand::new(operation, query, revision, intent);
        let Some(effect) = self.request(
            AppRequestPayload::WorkbenchCommand(command.clone()),
            PendingRequest::WorkbenchControl(command.clone()),
        ) else {
            return Vec::new();
        };
        self.chat.workbench.unresolved = Some((command, draft));
        if let Some(product) = &mut self.product {
            "Awaiting durable comment receipt; not yet accepted."
                .clone_into(&mut product.review.message);
        }
        vec![effect]
    }
}
