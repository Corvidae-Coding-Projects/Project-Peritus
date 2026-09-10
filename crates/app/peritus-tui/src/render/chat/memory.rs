//! Exact project-guidance projection with content-free tombstone disclosure.

use crate::model::{AppModel, format_digest, format_id};
use peritus_app_protocol::{WorkbenchGuidanceScope, WorkbenchGuidanceSource, WorkbenchMemoryRow};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    if let Some(memory) = &panel.memory {
        lines.push(Line::from(format!(
            "Dependency revision {} · rows {}–{} / {}{}",
            memory.dependency_revision(),
            if memory.rows().is_empty() { 0 } else { memory.query().offset() + 1 },
            memory.query().offset() as usize + memory.rows().len(),
            memory.total(),
            if memory.query().include_forgotten() { " · history" } else { "" }
        )));
        for row in memory.rows() {
            match row {
                WorkbenchMemoryRow::Active(record) => render_active(&mut lines, record),
                WorkbenchMemoryRow::Forgotten(tombstone) => {
                    lines.push(Line::from(format!(
                        "{} · forgotten · former r{} · scope={} · pinned={} · dependency r{}",
                        format_id(tombstone.identity().id().as_bytes()),
                        tombstone.prior().revision(),
                        scope(tombstone.prior().scope()),
                        tombstone.prior().pinned(),
                        tombstone.dependency_revision()
                    )));
                    lines.push(Line::from(format!(
                        "former SHA256 {} · forgotten by {} · reason: {}",
                        format_digest(tombstone.prior().digest().as_bytes()),
                        format_id(tombstone.forgotten_by().as_bytes()),
                        tombstone.reason().as_str()
                    )));
                    lines.push(Line::from("Content is intentionally absent from this tombstone."));
                }
            }
        }
    } else {
        lines.push(Line::from("No project-guidance page loaded."));
    }
    lines.extend([
        Line::from("Guidance is user-approved context, not authority or a reusable tool approval."),
        Line::from("Forget excludes future retrieval; it does not erase past prompts, journals, backups, or exports."),
        Line::from("Esc, then /memory save|revise|pin|unpin|project|conversation|forget · history|more|previous"),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Memory · project guidance "))
            .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0)),
        sections[0],
    );
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · r refresh"), sections[1]);
}

fn render_active(
    lines: &mut Vec<Line<'static>>,
    record: &peritus_app_protocol::WorkbenchGuidanceRecord,
) {
    lines.push(Line::from(format!(
        "{} · active r{} · dependency r{} · scope={} · pinned={}",
        format_id(record.identity().id().as_bytes()),
        record.version().record(),
        record.version().dependency(),
        scope(record.content().scope()),
        record.pinned()
    )));
    let source = match record.content().source() {
        WorkbenchGuidanceSource::UserAuthored => "user-authored".to_owned(),
        WorkbenchGuidanceSource::AcceptedPublicReply { operation, invocation, digest } => format!(
            "accepted reply {} after {} · SHA256 {}",
            format_id(operation.as_bytes()),
            format_id(invocation.as_bytes()),
            format_digest(digest.as_bytes())
        ),
    };
    lines.push(Line::from(format!(
        "source={source} · validated by {} · SHA256 {}",
        format_id(record.last_validation().operation().as_bytes()),
        format_digest(record.last_validation().content_digest().as_bytes())
    )));
    lines.push(Line::from(record.content().text().as_str().to_owned()));
}

fn scope(value: WorkbenchGuidanceScope) -> String {
    match value {
        WorkbenchGuidanceScope::Project => "project".to_owned(),
        WorkbenchGuidanceScope::Conversation(id) => {
            format!("conversation:{}", format_id(id.as_bytes()))
        }
    }
}
