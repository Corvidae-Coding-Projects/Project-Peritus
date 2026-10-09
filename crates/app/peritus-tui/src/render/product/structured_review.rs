//! Digest-bound structured diff, anchored comments, and evidence freshness rendering.

use peritus_app_protocol::{
    WorkbenchDiffFile, WorkbenchDiffLineKind, WorkbenchReviewComment, WorkbenchReviewCommentState,
    WorkbenchReviewEvidenceState, WorkbenchReviewPage,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::detail::safe;
use crate::{
    model::{AppModel, ReviewFocus, format_digest},
    render::{ACCENT, BAD, GOOD, MUTED},
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let Some(product) = &model.product else { return };
    let review = &product.review;
    let Some(page) = &review.page else { return };
    let [header, files, hunk, comments, evidence] = regions(area);
    render_header(frame, header, page, &review.message);
    render_files(frame, files, page, review.file, review.focus);
    render_hunk(
        frame,
        hunk,
        review.selected_file(),
        review.hunk,
        review.focus,
        if review.focus == ReviewFocus::Comment { 0 } else { review.scroll },
    );
    render_comments(
        frame,
        comments,
        page,
        review.comment,
        review.focus,
        if review.focus == ReviewFocus::Comment { review.scroll } else { 0 },
    );
    render_evidence(frame, evidence, page);
}

fn regions(area: Rect) -> [Rect; 5] {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(6)])
        .split(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(29), Constraint::Percentage(71)])
        .split(vertical[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
        .split(columns[1]);

    let lower = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(right[1]);
    [vertical[0], columns[0], right[0], lower[0], lower[1]]
}

pub(super) fn scroll_limit(model: &AppModel, area: Rect) -> u16 {
    let Some(product) = &model.product else { return 0 };
    let review = &product.review;
    let [_, _, hunk, comments, _] = regions(area);
    let (lines, area) = if review.focus == ReviewFocus::Comment {
        (comment_lines(review.selected_comment()), comments)
    } else {
        (selected_hunk_lines(review.selected_file(), review.hunk), hunk)
    };
    let lines =
        crate::render::chat::wrapped_lines(lines, usize::from(area.width.saturating_sub(2)));
    super::content_scroll_limit(lines.len(), area)
}

fn render_header(frame: &mut Frame<'_>, area: Rect, page: &WorkbenchReviewPage, message: &str) {
    let digest = format_digest(page.candidate_digest().as_bytes());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("Candidate {} · revision {} · ", &digest[..12], page.query().revision()),
                Style::default().fg(Color::White),
            ),
            Span::styled(safe(message), Style::default().fg(MUTED)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Review · Tab focus · ↑↓ select · PgUp/PgDn scroll · t raw · r refresh "),
        ),
        area,
    );
}

fn render_files(
    frame: &mut Frame<'_>,
    area: Rect,
    page: &WorkbenchReviewPage,
    selected: usize,
    focus: ReviewFocus,
) {
    let items = if page.files().is_empty() {
        vec![ListItem::new(Line::styled("No parsed file changes", Style::default().fg(MUTED)))]
    } else {
        page.files()
            .iter()
            .map(|file| {
                ListItem::new(format!(
                    "{}  {} hunks",
                    safe(file.anchor().path()),
                    file.hunks().len()
                ))
            })
            .collect()
    };
    let mut state = ListState::default();
    if !page.files().is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(focus_style(focus == ReviewFocus::File))
                    .title(" Files "),
            )
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        area,
        &mut state,
    );
}

fn render_hunk(
    frame: &mut Frame<'_>,
    area: Rect,
    file: Option<&WorkbenchDiffFile>,
    selected_hunk: usize,
    focus: ReviewFocus,
    scroll: u16,
) {
    let lines = crate::render::chat::wrapped_lines(
        selected_hunk_lines(file, selected_hunk),
        usize::from(area.width.saturating_sub(2)),
    );
    let maximum = super::content_scroll_limit(lines.len(), area);
    let target = if focus == ReviewFocus::File { "file" } else { "hunk" };
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(focus_style(focus == ReviewFocus::Hunk))
                    .title(format!(
                        " Target: {target} · e explain · v revise · K keep behavior · l leave alone "
                    )),
            )
            .scroll((scroll.min(maximum), 0)),
        area,
    );
}

fn selected_hunk_lines(
    file: Option<&WorkbenchDiffFile>,
    selected_hunk: usize,
) -> Vec<Line<'static>> {
    file.map_or_else(
        || vec![Line::styled("No structured hunk selected.", Style::default().fg(MUTED))],
        |file| hunk_lines(file, selected_hunk),
    )
}

fn hunk_lines(file: &WorkbenchDiffFile, selected_hunk: usize) -> Vec<Line<'static>> {
    let Some(hunk) = file.hunks().get(selected_hunk) else {
        return vec![Line::styled(
            "Whole-file target selected; this file has no visible hunks.",
            Style::default().fg(MUTED),
        )];
    };
    let mut lines = vec![Line::styled(
        safe(hunk.header()),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )];
    lines.extend(hunk.lines().iter().map(|line| {
        let (prefix, color) = match line.kind() {
            WorkbenchDiffLineKind::Context => (" ", Color::White),
            WorkbenchDiffLineKind::Removed => ("-", BAD),
            WorkbenchDiffLineKind::Added => ("+", GOOD),
            WorkbenchDiffLineKind::Metadata => ("\\", MUTED),
        };
        Line::styled(format!("{prefix}{}", safe(line.text())), Style::default().fg(color))
    }));
    lines
}

fn render_comments(
    frame: &mut Frame<'_>,
    area: Rect,
    page: &WorkbenchReviewPage,
    selected: usize,
    focus: ReviewFocus,
    scroll: u16,
) {
    let lines = crate::render::chat::wrapped_lines(
        comment_lines(page.comments().get(selected)),
        usize::from(area.width.saturating_sub(2)),
    );
    let maximum = super::content_scroll_limit(lines.len(), area);
    let number = if page.comments().is_empty() { 0 } else { selected + 1 };
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(focus_style(focus == ReviewFocus::Comment))
                    .title(format!(
                        " Comment {number}/{} · ↑↓ select · d dismiss · b rebind ",
                        page.comments().len()
                    )),
            )
            .scroll((scroll.min(maximum), 0)),
        area,
    );
}

fn comment_lines(comment: Option<&WorkbenchReviewComment>) -> Vec<Line<'static>> {
    let Some(comment) = comment else {
        return vec![Line::styled("No review comments", Style::default().fg(MUTED))];
    };
    let state = match comment.state() {
        WorkbenchReviewCommentState::Open => "open",
        WorkbenchReviewCommentState::Addressed => "addressed",
        WorkbenchReviewCommentState::Stale => "stale — b rebind",
        WorkbenchReviewCommentState::Dismissed => "dismissed",
    };
    let mut lines = vec![
        Line::from(format!("{:?} · {state}", comment.feedback())),
        Line::from(safe(comment.anchor().path())),
    ];
    lines.extend(safe(comment.message().as_str()).lines().map(|line| Line::from(line.to_owned())));
    lines
}

fn render_evidence(frame: &mut Frame<'_>, area: Rect, page: &WorkbenchReviewPage) {
    let evidence = page
        .evidence()
        .iter()
        .map(|item| {
            let state = match item.state() {
                WorkbenchReviewEvidenceState::Missing => "missing",
                WorkbenchReviewEvidenceState::Current => "current",
                WorkbenchReviewEvidenceState::Failed => "failed",
                WorkbenchReviewEvidenceState::Stale => "stale",
            };
            Line::from(format!("{:?}: {state}", item.kind()))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(evidence)
            .block(Block::default().borders(Borders::ALL).title(" Evidence freshness "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn focus_style(focused: bool) -> Style {
    Style::default().fg(if focused { ACCENT } else { MUTED })
}
