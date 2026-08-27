//! Ratatui rendering for every G2 interaction view.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};

use crate::model::{
    AppModel, ConnectionStatus, Editor, NoticeLevel, PromptItem, PromptPhase, View, format_digest,
    format_id,
};

const ACCENT: Color = Color::Rgb(196, 128, 255);
const MUTED: Color = Color::Rgb(135, 145, 160);
const GOOD: Color = Color::Rgb(92, 200, 142);
const WARN: Color = Color::Rgb(238, 190, 94);
const BAD: Color = Color::Rgb(244, 105, 125);

pub fn draw(frame: &mut Frame<'_>, model: &AppModel) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4), Constraint::Length(1)])
        .split(frame.area());
    render_tabs(frame, regions[0], model);
    match model.view {
        View::Runs | View::Diff | View::Review | View::Trace | View::Evolution => {
            render_event_view(frame, regions[1], model);
        }
        View::Terminal => render_terminal(frame, regions[1], model),
        View::Approvals => render_prompts(frame, regions[1], model),
        View::Help => render_help(frame, regions[1]),
    }
    render_status(frame, regions[2], model);
    if let Some(editor) = &model.editor {
        render_editor(frame, editor);
    }
}

fn render_tabs(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let titles = View::ALL
        .iter()
        .enumerate()
        .map(|(index, view)| Line::from(format!(" {}:{} ", index + 1, view.label())))
        .collect::<Vec<_>>();
    let selected = View::ALL.iter().position(|view| *view == model.view).unwrap_or(0);
    let title = match &model.connection {
        ConnectionStatus::Connecting => " Peritus · connecting ".to_owned(),
        ConnectionStatus::Online { server, downgraded } => {
            let suffix = if *downgraded { " · negotiated downgrade" } else { "" };
            format!(" Peritus · {server}{suffix} ")
        }
        ConnectionStatus::Disconnected(_) => " Peritus · disconnected ".to_owned(),
    };
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(title))
        .select(selected)
        .style(Style::default().fg(MUTED))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn render_event_view(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);
    let visible = model.visible_event_indices();
    let items = if visible.is_empty() {
        vec![ListItem::new(Line::styled(
            "No matching live events received",
            Style::default().fg(MUTED),
        ))]
    } else {
        visible
            .iter()
            .filter_map(|index| model.events.get(*index))
            .map(|record| ListItem::new(record.summary()))
            .collect()
    };
    let mut state = ListState::default();
    if !visible.is_empty() {
        let selected = model
            .selected_event
            .and_then(|selected| visible.iter().position(|index| *index == selected))
            .unwrap_or(0);
        state.select(Some(selected));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} events ", model.view.label())),
        )
        .highlight_symbol("▸ ")
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, sections[0], &mut state);

    let detail = model.selected_event_record().map_or_else(
        || {
            Text::from(vec![
                Line::from("No event selected."),
                Line::from("Use j/k or the arrow keys to inspect the live stream."),
            ])
        },
        |record| {
            Text::from(vec![
                field("Cursor", record.cursor.get().to_string()),
                field("Event", format_id(record.event_id.as_bytes())),
                field("Family", format!("{} ({})", record.family_name, record.family)),
                field("Schema", record.schema.to_string()),
                field("Attempt", record.attempt.to_string()),
                field("SHA-256", format_digest(&record.digest)),
                field("Frame bytes", record.byte_len.to_string()),
                Line::from(""),
                Line::styled(
                    "Bounded inert frame preview",
                    Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                ),
                Line::from(record.preview.clone()),
            ])
        },
    );
    frame.render_widget(
        Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title(" Exact observation "))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn render_terminal(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let title = model.terminal.as_ref().map_or_else(
        || " Terminal · detached ".to_owned(),
        |terminal| {
            format!(
                " Terminal · {:?} · input {} ",
                terminal.phase(),
                if terminal.capture_input() { "captured" } else { "released" }
            )
        },
    );
    let inner_height = usize::from(area.height.saturating_sub(2));
    let text = model.terminal.as_ref().map_or_else(
        || {
            Text::from(vec![
                Line::styled("No terminal is attached.", Style::default().fg(MUTED)),
                Line::from("Press a and enter a daemon-owned ProcessId to attach."),
            ])
        },
        |terminal| {
            Text::from(
                terminal
                    .visible_lines(inner_height)
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            )
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_prompts(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);
    let items = if model.prompts.is_empty() {
        vec![ListItem::new(Line::styled("No prompts", Style::default().fg(MUTED)))]
    } else {
        model
            .prompts
            .iter()
            .map(|item| {
                let correlation = item.binding.correlation();
                ListItem::new(format!(
                    "{:?}  {:?}  {}",
                    item.binding.kind(),
                    item.phase,
                    short_id(correlation.prompt_id().as_bytes())
                ))
            })
            .collect()
    };
    let mut state = ListState::default();
    if !model.prompts.is_empty() {
        state.select(Some(model.selected_prompt.min(model.prompts.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Awaiting authority/input "))
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        sections[0],
        &mut state,
    );
    let detail = model
        .selected_prompt_item()
        .map_or_else(|| Text::from("No prompt selected."), prompt_detail);
    frame.render_widget(
        Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title(" Exact prompt binding "))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn prompt_detail(item: &PromptItem) -> Text<'static> {
    let binding = &item.binding;
    let correlation = binding.correlation();
    let revision = correlation.revision();
    let mut lines = vec![
        field("Kind", format!("{:?}", binding.kind())),
        field("Local phase", format!("{:?}", item.phase)),
        field("Prompt", format_id(correlation.prompt_id().as_bytes())),
        field("Origin request", format_id(correlation.originating_request_id().as_bytes())),
        field("Actor", format_id(correlation.actor_id().as_bytes())),
        field("Workspace", format_id(revision.workspace_id().as_bytes())),
        field(
            "Revision",
            format!(
                "generation {}, revision {}",
                revision.workspace_generation().get(),
                revision.workspace_revision().get()
            ),
        ),
        field("Freshness", format_digest(correlation.freshness_digest().as_bytes())),
    ];
    if let Some(challenge) = binding.approval_challenge() {
        lines
            .push(field("Decision command", format_id(challenge.decision_command_id().as_bytes())));
        lines.push(field("Registry revision", challenge.registry_revision().get().to_string()));
        lines.push(field(
            "Signing challenge",
            format!("{} canonical bytes", challenge.request_frame().len()),
        ));
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "Approval and denial both require an externally signed B1 decision. This client never manufactures authority.",
            Style::default().fg(WARN),
        ));
    }
    if !binding.choices().is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::styled("Choices", Style::default().fg(MUTED)));
        lines.extend(
            binding
                .choices()
                .iter()
                .map(|choice| Line::from(format!("  {} — {}", choice.id(), choice.label()))),
        );
    }
    if !binding.constraints().is_empty() {
        lines.push(Line::from(""));
        lines.push(field("Constraints", format!("{:?}", binding.constraints())));
    }
    lines.push(Line::from(""));
    lines.push(match item.phase {
        PromptPhase::Pending => Line::styled(
            "Enter: respond   c: cancel",
            Style::default().fg(GOOD).add_modifier(Modifier::BOLD),
        ),
        PromptPhase::Submitting => {
            Line::styled("Awaiting daemon response", Style::default().fg(WARN))
        }
        PromptPhase::Accepted => {
            Line::styled("Daemon accepted the protocol input", Style::default().fg(GOOD))
        }
        PromptPhase::Failed => {
            Line::styled("Daemon rejected the response", Style::default().fg(BAD))
        }
    });
    Text::from(lines)
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        Line::styled("Navigation", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Line::from("  1–7        open Runs, Diff, Review, Trace, Evolution, Terminal, Approvals"),
        Line::from("  Tab/Shift-Tab  next/previous view"),
        Line::from("  j/k or ↑/↓    select an event or prompt"),
        Line::from("  ?              this help"),
        Line::from(""),
        Line::styled("Live connection", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Line::from("  r              reconnect/resume durable session"),
        Line::from("  p/u            pause/resume event delivery"),
        Line::from("  Ctrl-Q/Ctrl-C  exit and send bounded detach/cancel cleanup"),
        Line::from(""),
        Line::styled("Terminal", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Line::from("  a              attach by exact ProcessId"),
        Line::from("  i              capture keyboard for attached terminal"),
        Line::from("  Ctrl-]         release terminal keyboard capture"),
        Line::from("  PageUp/Down    scroll transcript while capture is released"),
        Line::from("  d/x            detach / request process cancellation"),
        Line::from(""),
        Line::styled(
            "Authority boundary",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Line::from("  Approval decisions must arrive as externally signed canonical B1 frames."),
        Line::from(
            "  A TUI acknowledgement means protocol input only; daemon/domain state remains authoritative.",
        ),
        Line::from(
            "  Terminal and domain bytes are rendered as inert data with control sequences removed.",
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Key reference "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_status(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let (connection, style) = match &model.connection {
        ConnectionStatus::Connecting => ("connecting".to_owned(), Style::default().fg(WARN)),
        ConnectionStatus::Online { .. } => ("online".to_owned(), Style::default().fg(GOOD)),
        ConnectionStatus::Disconnected(error) => {
            (format!("offline: {error}"), Style::default().fg(BAD))
        }
    };
    let readiness = model.daemon_status.as_ref().map_or_else(
        || "readiness unknown".to_owned(),
        |status| format!("{:?}", status.readiness()),
    );
    let mut spans = vec![
        Span::styled(format!(" {connection} "), style.add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {readiness} "), Style::default().fg(MUTED)),
        Span::styled(format!(" cursor {} ", model.last_cursor().get()), Style::default().fg(MUTED)),
        Span::styled(
            format!(" session {} ", short_text(&model.session_label(), 12)),
            Style::default().fg(MUTED),
        ),
    ];
    if let Some(notice) = &model.notice {
        let color = match notice.level {
            NoticeLevel::Info => GOOD,
            NoticeLevel::Warning => WARN,
            NoticeLevel::Error => BAD,
        };
        spans.push(Span::styled(format!(" {} ", notice.text), Style::default().fg(color)));
    } else {
        spans.push(Span::styled(" ? help · Ctrl-Q quit ", Style::default().fg(MUTED)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_editor(frame: &mut Frame<'_>, editor: &Editor) {
    let area = centered(frame.area(), 76, 9);
    frame.render_widget(Clear, area);
    let content = Text::from(vec![
        Line::styled(editor.hint, Style::default().fg(MUTED)),
        Line::from(""),
        Line::styled(
            if editor.buffer.is_empty() { " " } else { editor.buffer.as_str() },
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled("Enter submit · Esc cancel", Style::default().fg(MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT))
                    .title(format!(" {} ", editor.title))
                    .title_alignment(Alignment::Center),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn centered(area: Rect, width_percent: u16, height: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(height) / 2),
            Constraint::Length(height.min(area.height)),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

fn field(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<18}"), Style::default().fg(MUTED)),
        Span::raw(value),
    ])
}

fn short_id(bytes: &[u8; 16]) -> String {
    short_text(&format_id(bytes), 12)
}

fn short_text(text: &str, maximum: usize) -> String {
    if text.len() <= maximum { text.to_owned() } else { format!("{}…", &text[..maximum]) }
}
