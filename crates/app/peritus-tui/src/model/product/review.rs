//! Interactive structured-diff selection and revision-fenced comment controls.

use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{
    AppRequestPayload, ControlOperationId, WellKnownProtocolFeature, WorkbenchCommand,
    WorkbenchIntent, WorkbenchReviewCommentState, WorkbenchReviewFeedback, WorkbenchReviewPage,
    WorkbenchReviewQuery,
};

use super::ProductUi;
use crate::{
    action::Effect,
    model::{AppModel, Editor, EditorKind, NoticeLevel, PendingRequest, View},
};

mod state;

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
        if !run.phase().terminal() {
            if let Some(product) = &mut self.product {
                product.review.page = None;
                "Structured comments open at a terminal effect boundary; raw diff remains available."
                    .clone_into(&mut product.review.message);
            }
            return Vec::new();
        }
        let request = WorkbenchReviewQuery::new(query, run.run_id(), 0, 0);
        self.request(
            AppRequestPayload::QueryWorkbenchReview(request),
            PendingRequest::WorkbenchReview(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_review_page(
        &mut self,
        requested: WorkbenchReviewQuery,
        page: WorkbenchReviewPage,
    ) {
        let selected = self
            .product
            .as_ref()
            .and_then(ProductUi::selected_run)
            .map(peritus_app_protocol::ProductRunSnapshot::run_id);
        if page.query().query() != requested.query()
            || page.query().run() != requested.run()
            || page.query().offset() != requested.offset()
            || selected != Some(requested.run())
            || self.chat.workbench.selected != Some(requested.query())
        {
            self.notice(NoticeLevel::Error, "Mismatched structured-review response ignored.");
            return;
        }
        if let Some(product) = &mut self.product {
            let review = &mut product.review;
            review.file = review.file.min(page.files().len().saturating_sub(1));
            review.hunk = review.hunk.min(
                page.files()
                    .get(review.file)
                    .map_or(0, |file| file.hunks().len().saturating_sub(1)),
            );
            review.comment = review.comment.min(page.comments().len().saturating_sub(1));
            review.message = format!(
                "Revision {} · {} comments · anchors are digest-bound",
                page.query().revision(),
                page.total_comments(),
            );
            review.page = Some(page);
        }
    }

    pub(in crate::model) fn review_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        let structured = self.product.as_ref().is_some_and(|product| product.review.page.is_some());
        match key.code {
            KeyCode::Char('t') => {
                if let Some(product) = &mut self.product {
                    product.review.raw = !product.review.raw;
                    product.inspection_scroll = 0;
                }
                Some(Vec::new())
            }
            KeyCode::Char('r') => Some(self.refresh_review()),
            _ if !structured => None,
            KeyCode::Tab => {
                if let Some(product) = &mut self.product {
                    product.review.focus = match product.review.focus {
                        ReviewFocus::File => ReviewFocus::Hunk,
                        ReviewFocus::Hunk => ReviewFocus::Comment,
                        ReviewFocus::Comment => ReviewFocus::File,
                    };
                }
                Some(Vec::new())
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_review_selection(false);
                Some(Vec::new())
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_review_selection(true);
                Some(Vec::new())
            }
            KeyCode::Left => {
                if let Some(product) = &mut self.product {
                    product.review.hunk = product.review.hunk.saturating_sub(1);
                }
                Some(Vec::new())
            }
            KeyCode::Right => {
                if let Some(product) = &mut self.product {
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

    fn move_review_selection(&mut self, forward: bool) {
        let Some(product) = &mut self.product else { return };
        let review = &mut product.review;
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
    }

    fn open_review_editor(&mut self, feedback: WorkbenchReviewFeedback) -> Vec<Effect> {
        if self.product.as_ref().and_then(|product| product.review.selected_anchor()).is_none() {
            self.notice(NoticeLevel::Warning, "Select a structured file or hunk first.");
            return Vec::new();
        }
        let (title, hint) = match feedback {
            WorkbenchReviewFeedback::Explain => (
                "Explain selected change",
                "Ask a source-grounded question. This does not permit mutation.",
            ),
            WorkbenchReviewFeedback::RequestRevision => (
                "Request selected revision",
                "Describe the desired revision. Ordinary admission and requalification still apply.",
            ),
            WorkbenchReviewFeedback::KeepBehavior => (
                "Keep selected behavior",
                "Describe the behavior to preserve; this is a semantic preference.",
            ),
            WorkbenchReviewFeedback::LeaveAlone => (
                "Protect selected path",
                "Describe what must remain untouched. The host blocks writes to the complete path.",
            ),
        };
        self.editor = Some(Editor {
            kind: EditorKind::ReviewFeedback(feedback),
            title,
            hint,
            buffer: String::new(),
            cursor: 0,
        });
        Vec::new()
    }

    pub(in crate::model) fn submit_review_feedback(
        &mut self,
        feedback: WorkbenchReviewFeedback,
        message: String,
    ) -> Vec<Effect> {
        let Ok(text) = peritus_app_protocol::WorkbenchInputText::new(message.clone()) else {
            self.editor = Some(Editor {
                kind: EditorKind::ReviewFeedback(feedback),
                title: "Review comment",
                hint: "Enter a nonempty bounded comment.",
                cursor: message.len(),
                buffer: message,
            });
            self.notice(
                NoticeLevel::Warning,
                "Review comment must be nonempty and within the input limit; draft retained.",
            );
            return Vec::new();
        };
        let Some((query, revision, anchor)) = self.product.as_ref().and_then(|product| {
            let page = product.review.page.as_ref()?;
            Some((
                page.query().query(),
                page.query().revision(),
                product.review.selected_anchor()?.clone(),
            ))
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Refresh and select a current structured target; draft retained.",
            );
            self.editor = Some(Editor {
                kind: EditorKind::ReviewFeedback(feedback),
                title: "Review comment",
                hint: "Refresh before retrying this draft.",
                cursor: message.len(),
                buffer: message,
            });
            return Vec::new();
        };
        self.submit_review_intent(
            query,
            revision,
            WorkbenchIntent::AddReview { anchor, feedback, message: text },
            message,
        )
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
