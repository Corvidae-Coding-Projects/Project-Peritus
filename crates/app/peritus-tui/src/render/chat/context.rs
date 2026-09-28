//! Content-free context inspector. The composer retains a separate non-overlapping area.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::{
    WorkbenchContextDisposition as D, WorkbenchContextPreference as P, WorkbenchContextSource as S,
    WorkbenchContextView as V,
};
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Context · read only ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(page) = &panel.context_page {
        lines.push(format!(
            "Revision {} · rows {}–{} / {}",
            page.query().revision(),
            if page.rows().is_empty() { 0 } else { page.query().offset() + 1 },
            page.query().offset() as usize + page.rows().len(),
            page.total()
        ));
        lines.push(String::from(match page.query().view() {
            V::Next => "Eligible inputs only; the next complete model request is not sealed yet.",
            V::History => "Sealed request bindings; incorporation is not proof of provider delivery.",
            V::Invocation(_) => "Exact sealed source manifest. Message content and private reasoning are not exposed.",
        }));
        if let Some(seal) = page.seal() {
            lines.push(format!(
                "Invocation {} · input generation {}",
                format_id(seal.invocation().as_bytes()),
                seal.generation()
            ));
            lines.push(format!("Request SHA256 {}", hex(seal.request_digest().as_bytes())));
            lines.push(format!("Manifest SHA256 {}", hex(seal.manifest_digest().as_bytes())));
        }
        lines.push(String::from("Token count: unknown (no tokenizer/provider report). Source sizes are not additive: message rows may contain the input rows."));
        for (i, row) in page.rows().iter().enumerate() {
            let source = match row.source() {
                S::File { attachment, version } => format!(
                    "File {} · version {}",
                    format_id(attachment.as_bytes()),
                    format_id(version.as_bytes())
                ),
                S::Image { operation, input, artifact } => format!(
                    "Image {} · caption {} · artifact {}",
                    format_id(operation.as_bytes()),
                    format_id(input.as_bytes()),
                    format_id(artifact.as_bytes())
                ),
                S::Input(selected) => format!(
                    "User input {} · content r{}",
                    format_id(selected.id().as_bytes()),
                    selected.revision()
                ),
                S::PublicReply(id) => format!("Public reply after {}", format_id(id.as_bytes())),
                S::Message { ordinal, role } => {
                    format!("Request message {} · {role:?} · C5 encoded", ordinal + 1)
                }
                S::Invocation { id, .. } => format!("Invocation {}", format_id(id.as_bytes())),
            };
            let status = match row.disposition() {
                D::Eligible => "eligible; not sealed",
                D::Included => "included in sealed request",
                D::Held => "excluded: held",
                D::Withdrawn => "excluded: withdrawn",
                D::Superseded => "excluded: superseded",
                D::DependencyBlocked => "excluded: prerequisite unavailable",
                D::AwaitingLaterInput => "excluded until later user input",
                D::Deselected => "excluded: attachment deselected",
                D::UserExcluded => "excluded: user context preference",
            };
            lines.push(format!("{}. {source}", page.query().offset() as usize + i + 1));
            lines.push(format!("{status} · {} bytes", row.bytes()));
            if let Some(preference) = row.preference() {
                lines.push(String::from(match preference {
                    P::Pinned => "Preference: pinned",
                    P::Excluded => "Preference: excluded",
                }));
            }
            lines.push(format!("SHA256 {}", hex(row.digest().as_bytes())));
            if let S::Invocation { manifest_digest, .. } = row.source() {
                lines.push(format!("Manifest SHA256 {}", hex(manifest_digest.as_bytes())));
            }
        }
    } else {
        lines.push(String::from("No context page loaded."));
    }
    lines.push(String::from(
        "Esc then /context next | pin/exclude/default <row> | history | show <invocation ID> | more | previous",
    ));
    lines
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
