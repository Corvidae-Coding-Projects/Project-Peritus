//! Structured-diff review selection state.

use peritus_app_protocol::{
    WorkbenchDiffFile, WorkbenchReviewAnchor, WorkbenchReviewComment,
    WorkbenchReviewDiffBytesQuery, WorkbenchReviewDiffPage, WorkbenchReviewDiffQuery,
    WorkbenchReviewPage, WorkbenchReviewQuery,
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
    pub pending: Option<WorkbenchReviewQuery>,
    pub diff_page: Option<WorkbenchReviewDiffPage>,
    pub pending_diff: Option<WorkbenchReviewDiffQuery>,
    pub pending_raw: Option<WorkbenchReviewDiffBytesQuery>,
    pub raw_line: Option<(u32, Vec<u8>)>,
    pub raw_stream: bool,
    pub raw_total_bytes: Option<u32>,
    pub raw_lines: Vec<(u32, u32)>,
    pub raw_index: usize,
    pub diff_history: Vec<WorkbenchReviewDiffQuery>,
    pub raw: bool,
    pub focus: ReviewFocus,
    pub file: usize,
    pub hunk: usize,
    pub comment: usize,
    pub scroll: usize,
    pub message: String,
}

impl DiffReviewUi {
    pub fn clear(&mut self) {
        self.page = None;
        self.pending = None;
        self.diff_page = None;
        self.pending_diff = None;
        self.pending_raw = None;
        self.raw_line = None;
        self.raw_stream = false;
        self.raw_total_bytes = None;
        self.raw_lines.clear();
        self.raw_index = 0;
        self.diff_history.clear();
        self.file = 0;
        self.hunk = 0;
        self.comment = 0;
        self.scroll = 0;
        self.message.clear();
    }

    pub fn selected_file(&self) -> Option<&WorkbenchDiffFile> {
        self.page.as_ref()?.files().get(self.file)
    }

    pub fn selected_anchor(&self) -> Option<&WorkbenchReviewAnchor> {
        if let Some(page) = &self.diff_page {
            return Some(match self.focus {
                ReviewFocus::File => page.file_anchor(),
                ReviewFocus::Hunk | ReviewFocus::Comment => page.hunk().map_or_else(
                    || page.file_anchor(),
                    peritus_app_protocol::WorkbenchDiffHunk::anchor,
                ),
            });
        }
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
