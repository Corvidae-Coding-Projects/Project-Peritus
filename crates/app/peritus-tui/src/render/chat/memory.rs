//! Exact project-guidance projection with content-free tombstone disclosure.

use crate::model::{AppModel, format_digest, format_id};
use peritus_app_protocol::{WorkbenchGuidanceScope, WorkbenchGuidanceSource, WorkbenchMemoryRow};
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Memory · project guidance ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(memory) = &panel.memory {
        lines.push(format!(
            "Dependency revision {} · rows {}–{} / {}{}",
            memory.dependency_revision(),
            if memory.rows().is_empty() { 0 } else { memory.query().offset() + 1 },
            memory.query().offset() as usize + memory.rows().len(),
            memory.total(),
            if memory.query().include_forgotten() { " · history" } else { "" }
        ));
        for row in memory.rows() {
            match row {
                WorkbenchMemoryRow::Active(record) => render_active(&mut lines, record),
                WorkbenchMemoryRow::Forgotten(tombstone) => {
                    lines.push(format!(
                        "{} · forgotten · former r{} · scope={} · pinned={} · dependency r{}",
                        format_id(tombstone.identity().id().as_bytes()),
                        tombstone.prior().revision(),
                        scope(tombstone.prior().scope()),
                        tombstone.prior().pinned(),
                        tombstone.dependency_revision()
                    ));
                    lines.push(format!(
                        "former SHA256 {} · forgotten by {} · reason: {}",
                        format_digest(tombstone.prior().digest().as_bytes()),
                        format_id(tombstone.forgotten_by().as_bytes()),
                        tombstone.reason().as_str()
                    ));
                    lines
                        .push(String::from("Content is intentionally absent from this tombstone."));
                }
            }
        }
    } else {
        lines.push(String::from("No project-guidance page loaded."));
    }
    lines.extend([
        String::from("Guidance is user-approved context, not authority or a reusable tool approval."),
        String::from("Forget excludes future retrieval; it does not erase past prompts, journals, backups, or exports."),
        String::from("Esc, then /memory save|revise|pin|unpin|project|conversation|forget · history|more|previous"),
    ]);
    lines
}

fn render_active(lines: &mut Vec<String>, record: &peritus_app_protocol::WorkbenchGuidanceRecord) {
    lines.push(format!(
        "{} · active r{} · dependency r{} · scope={} · pinned={}",
        format_id(record.identity().id().as_bytes()),
        record.version().record(),
        record.version().dependency(),
        scope(record.content().scope()),
        record.pinned()
    ));
    let source = match record.content().source() {
        WorkbenchGuidanceSource::UserAuthored => "user-authored".to_owned(),
        WorkbenchGuidanceSource::AcceptedPublicReply { operation, invocation, digest } => format!(
            "accepted reply {} after {} · SHA256 {}",
            format_id(operation.as_bytes()),
            format_id(invocation.as_bytes()),
            format_digest(digest.as_bytes())
        ),
    };
    lines.push(format!(
        "source={source} · validated by {} · SHA256 {}",
        format_id(record.last_validation().operation().as_bytes()),
        format_digest(record.last_validation().content_digest().as_bytes())
    ));
    lines.push(record.content().text().as_str().to_owned());
}

fn scope(value: WorkbenchGuidanceScope) -> String {
    match value {
        WorkbenchGuidanceScope::Project => "project".to_owned(),
        WorkbenchGuidanceScope::Conversation(id) => {
            format!("conversation:{}", format_id(id.as_bytes()))
        }
    }
}
