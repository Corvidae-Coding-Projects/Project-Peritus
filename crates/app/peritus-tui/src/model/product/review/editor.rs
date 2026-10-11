//! Review drafts retain the exact target selected when editing began.

use crate::{
    action::Effect,
    model::{AppModel, Editor, EditorKind, NoticeLevel},
};
use peritus_app_protocol::{
    WorkbenchIntent, WorkbenchQuery, WorkbenchReviewAnchor, WorkbenchReviewFeedback,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewDraft {
    pub query: WorkbenchQuery,
    pub anchor: WorkbenchReviewAnchor,
    pub feedback: WorkbenchReviewFeedback,
}

impl AppModel {
    pub(super) fn open_review_editor(&mut self, feedback: WorkbenchReviewFeedback) -> Vec<Effect> {
        let Some(target) = self.review_draft_target(feedback) else {
            self.notice(NoticeLevel::Warning, "Select a structured file or hunk first.");
            return Vec::new();
        };
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
        self.open_editor(Editor {
            kind: EditorKind::ReviewFeedback(Box::new(target)),
            title,
            hint,
            buffer: String::new(),
            cursor: 0,
            pasted_command: false,
        });
        Vec::new()
    }

    pub(in crate::model) fn submit_review_feedback(
        &mut self,
        target: ReviewDraft,
        message: String,
    ) -> Vec<Effect> {
        let Ok(text) = peritus_app_protocol::WorkbenchInputText::new(message.clone()) else {
            self.notice(
                NoticeLevel::Warning,
                "Review comment must be nonempty and within the input limit; draft retained.",
            );
            return Vec::new();
        };
        let revision = self.product.as_ref().and_then(|product| {
            let page = product.review.page.as_ref()?;
            let anchor_is_current = product.review.diff_page.as_ref().map_or_else(
                || {
                    page.files().iter().any(|file| {
                        file.anchor() == &target.anchor
                            || file.hunks().iter().any(|hunk| hunk.anchor() == &target.anchor)
                    })
                },
                |diff| {
                    diff.query().query() == target.query
                        && (diff.file_anchor() == &target.anchor
                            || diff.hunk().is_some_and(|hunk| hunk.anchor() == &target.anchor))
                },
            );
            (anchor_is_current
                && page.query().query() == target.query
                && self.chat.workbench.selected == Some(target.query)
                && product.review.diff_page.as_ref().is_none_or(|diff| {
                    diff.query().revision() == page.query().revision()
                        && diff.candidate_digest() == page.candidate_digest()
                }))
            .then_some(page.query().revision())
        });
        let Some(revision) = revision else {
            self.notice(
                NoticeLevel::Warning,
                "Review target changed. Ctrl-F refreshes; Ctrl-B explicitly rebinds to the current selection. Draft retained.",
            );
            return Vec::new();
        };
        self.submit_review_intent(
            target.query,
            revision,
            WorkbenchIntent::AddReview {
                anchor: target.anchor,
                feedback: target.feedback,
                message: text,
            },
            message,
        )
    }

    fn review_draft_target(&self, feedback: WorkbenchReviewFeedback) -> Option<ReviewDraft> {
        let review = &self.product.as_ref()?.review;
        let query = review.page.as_ref()?.query().query();
        (self.chat.workbench.selected == Some(query)).then_some(ReviewDraft {
            query,
            anchor: review.selected_anchor()?.clone(),
            feedback,
        })
    }

    pub(in crate::model) fn rebind_review_draft(&mut self) {
        let Some(Editor { kind: EditorKind::ReviewFeedback(original), .. }) = &self.editor else {
            return;
        };
        let Some(target) = self.review_draft_target(original.feedback) else {
            self.notice(
                NoticeLevel::Warning,
                "No current review selection. Ctrl-F refreshes; draft retained.",
            );
            return;
        };
        let path = target.anchor.path().to_owned();
        if let Some(editor) = &mut self.editor {
            editor.kind = EditorKind::ReviewFeedback(Box::new(target));
        }
        self.notice(
            NoticeLevel::Info,
            format!("Draft rebound to current selection: {path}. Enter submits."),
        );
    }
}
