//! Explicit local conveniences. These operations never call a model or grant daemon authority.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardSource {
    Clipboard,
    Primary,
}

#[derive(Clone, Debug)]
pub enum LocalOperation {
    FindFiles { root: PathBuf, query: String },
    SearchTranscript { scope: String, query: String },
    ReadClipboardImage,
    ReadClipboardText(ClipboardSource),
    EditDraft { text: String },
}

#[derive(Debug)]
pub enum LocalReply {
    Files(Vec<PathBuf>),
    Transcript(crate::transcript_archive::SearchPage),
    ClipboardImage(PathBuf),
    ClipboardText(String),
    EditedDraft(String),
}
