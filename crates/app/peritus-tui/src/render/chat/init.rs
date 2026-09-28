//! Exact initialization source, command, and file-diff review panel.

use crate::model::AppModel;
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Init · reviewed diff ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r rediscover",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(proposal) = &panel.init {
        lines.push(format!(
            "Conversation revision {} · {} observed source(s)",
            proposal.revision(),
            proposal.sources().len()
        ));
        for source in proposal.sources() {
            lines.push(format!(
                "source {} · {:?} · {} bytes",
                source.path(),
                source.kind(),
                source.bytes()
            ));
        }
        lines.push(String::from("Discovered commands · inert and unverified"));
        if proposal.commands().is_empty() {
            lines.push(String::from("No command candidates discovered."));
        }
        for command in proposal.commands() {
            lines.push(format!(
                "{} · {} {} · from {} · {:?}",
                command.kind().as_str(),
                command.executable(),
                command.arguments().join(" "),
                command.source(),
                command.verification()
            ));
        }
        lines.push(String::from("Exact reviewed instruction-file diff"));
        lines.extend(proposal.patch().diff().lines().map(str::to_owned));
    } else {
        lines.push(String::from("No initialization proposal loaded."));
    }
    lines.extend([
        String::from(
            "Applying authorizes only this exact file patch; it never executes listed commands.",
        ),
        String::from("Esc, then /init apply · /init decline leaves the workspace unchanged"),
    ]);
    lines
}
