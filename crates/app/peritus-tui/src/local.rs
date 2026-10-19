//! Explicit local conveniences. These operations never call a model or grant daemon authority.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) enum LocalOperation {
    FindFiles { root: PathBuf, query: String },
    SearchTranscript { scope: String, query: String },
    ReadClipboardImage,
    EditDraft { text: String },
}

#[derive(Debug)]
pub(crate) enum LocalReply {
    Files(Vec<PathBuf>),
    Transcript(crate::transcript_archive::SearchPage),
    ClipboardImage(PathBuf),
    EditedDraft(String),
}
