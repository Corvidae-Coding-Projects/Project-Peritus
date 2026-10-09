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
    if let Some(proposal) = panel.init_artifact {
        lines.push(format!(
            "Conversation revision {} · complete review {} bytes",
            proposal.request().revision(),
            proposal.review().bytes()
        ));
        if let Some(page) = &panel.init_artifact_page {
            lines.push(format!(
                "Review bytes {}..{} of {} (UTF-8 boundary fragments are escaped)",
                page.request().offset(),
                page.request().offset() + page.bytes().len() as u64,
                proposal.review().bytes()
            ));
            lines.extend(exact_page_text(page.bytes()).lines().map(str::to_owned));
        } else {
            lines.push("Loading exact review page...".to_owned());
        }
        lines.push(
            "/init next · /init previous · /init command <number> toggles a command".to_owned(),
        );
        lines.push(
            "/init source <manifest|commands|docs|instructions> <path> adds an explicit source"
                .to_owned(),
        );
    } else if let Some(proposal) = &panel.init {
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

fn exact_page_text(mut bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(rest) => {
                text.push_str(rest);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if let Ok(prefix) = std::str::from_utf8(&bytes[..valid]) {
                    text.push_str(prefix);
                }
                let invalid = error.error_len().unwrap_or(bytes.len() - valid);
                for byte in &bytes[valid..valid + invalid] {
                    write!(&mut text, "\\x{byte:02x}").expect("writing to String");
                }
                bytes = &bytes[valid + invalid..];
            }
        }
    }
    text
}
