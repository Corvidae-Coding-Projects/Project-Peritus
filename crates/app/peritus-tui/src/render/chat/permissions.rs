//! Effective host-intersected workspace permission projection.

use crate::model::AppModel;
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Permissions · enforced ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(permissions) = &panel.permissions {
        lines.push(format!(
            "{} · conversation revision {} · policy revision {}",
            permissions.trust().as_str(),
            permissions.conversation_revision(),
            permissions.authority_revision()
        ));
        for entry in permissions.entries() {
            let approval = if entry.explicit_approval_still_required() {
                " · exact operation approval still required"
            } else {
                ""
            };
            lines.push(format!(
                "{:<7} effective={} · host={} · {}{}",
                entry.capability().as_str(),
                state(entry.effective_allowed()),
                state(entry.host_allowed()),
                entry.provenance().as_str(),
                approval
            ));
        }
    } else {
        lines.push(String::from("No permission inspection loaded."));
    }
    lines.extend([
        String::from("This overlay can narrow host authority, never broaden it."),
        String::from(
            "Every lower path, lease, sandbox, and signed-operation gate remains mandatory.",
        ),
        String::from("Esc, then /permissions restrict|grant read|write|process|network"),
    ]);
    lines
}

const fn state(allowed: bool) -> &'static str {
    if allowed { "allowed" } else { "denied" }
}
