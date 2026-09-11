//! Structured-diff review selection state.

use peritus_app_protocol::{
    WorkbenchDiffFile, WorkbenchReviewAnchor, WorkbenchReviewComment, WorkbenchReviewPage,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReviewFocus {
    #[default]
    File,
    Hunk,
    Comment,
}

#[derive(Debug, Default)]
pub struct DiffReviewUi {
    pub page: Option<WorkbenchReviewPage>,
    pub raw: bool,
    pub focus: ReviewFocus,
    pub file: usize,
    pub hunk: usize,
    pub comment: usize,
    pub message: String,
}

impl DiffReviewUi {
    pub fn clear(&mut self) {
        self.page = None;
        self.file = 0;
        self.hunk = 0;
        self.comment = 0;
        self.message.clear();
    }

    pub fn selected_file(&self) -> Option<&WorkbenchDiffFile> {
        self.page.as_ref()?.files().get(self.file)
    }

    pub fn selected_anchor(&self) -> Option<&WorkbenchReviewAnchor> {
        let file = self.selected_file()?;
        match self.focus {
            ReviewFocus::File => Some(file.anchor()),
            ReviewFocus::Hunk | ReviewFocus::Comment => Some(
                file.hunks().get(self.hunk).map_or_else(|| file.anchor(), |hunk| hunk.anchor()),
            ),
        }
    }

    pub fn selected_comment(&self) -> Option<&WorkbenchReviewComment> {
        self.page.as_ref()?.comments().get(self.comment)
    }
}
