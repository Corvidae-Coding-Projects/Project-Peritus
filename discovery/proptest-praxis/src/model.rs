//! Discovery-only minimal shell for the real TUI bounded paste implementation.
//!
//! The source-mounted implementation below is the checked-in TUI method body. The shell
//! supplies only the fields that method reads or writes; it is not a model of the full UI.

use peritus_app_protocol::AppProtocolLimits;

#[derive(Clone, Copy)]
pub(crate) enum EditorKind {
    ProductMessage(()),
    ReviewFeedback(()),
    ProcessId,
    ApprovalSignature(()),
    PromptAnswer(()),
}

pub(crate) struct Editor {
    pub kind: EditorKind,
    pub buffer: String,
    pub cursor: usize,
    pub pasted_command: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum NoticeLevel {
    Warning,
}

pub(crate) struct AppModel {
    pub editor: Option<Editor>,
    pub limits: AppProtocolLimits,
    pub warning: Option<String>,
}

impl AppModel {
    fn notice(&mut self, _level: NoticeLevel, text: String) {
        self.warning = Some(text);
    }
}

#[path = "../../../crates/app/peritus-tui/src/model/editor/input.rs"]
mod production_paste;

pub(crate) fn paste(model: &mut AppModel, text: &str) {
    model.paste_editor_event(text);
}

pub(crate) fn type_text(model: &mut AppModel, text: &str) {
    let _ = model.paste_editor(text);
}
