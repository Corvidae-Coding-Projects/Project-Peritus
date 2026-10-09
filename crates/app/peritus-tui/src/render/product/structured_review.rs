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
use std::fmt::Write as _;

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
    render_files(frame, files, page, review.diff_page.as_ref(), review.file, review.focus);
    render_hunk(
        frame,
        hunk,
        review.diff_page.as_ref(),
        review.raw_line.as_ref(),
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

pub(super) fn render_raw_stream(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let Some(review) = model.product.as_ref().map(|product| &product.review) else { return };
    let content = review.raw_line.as_ref().map_or_else(
        || "Loading exact raw diff range…".to_owned(),
        |(offset, bytes)| format!("[{}..] {}", offset, raw_byte_preview(bytes)),
    );
    let total = review.raw_total_bytes.unwrap_or(0);
    let offset = review.raw_line.as_ref().map_or(0, |(offset, _)| *offset);
    frame.render_widget(
        Paragraph::new(safe(&content))
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(format!(
                " Raw diff · bytes {offset} / {total} · [ ] page ranges · t structured "
            )))
            .scroll((review.scroll, 0)),
        area,
    );
}

pub(super) fn raw_stream_scroll_limit(model: &AppModel, area: Rect) -> u16 {
    let Some(review) = model.product.as_ref().map(|product| &product.review) else { return 0 };
    let content = review.raw_line.as_ref().map_or_else(
        || "Loading exact raw diff range…".to_owned(),
        |(offset, bytes)| format!("[{}..] {}", offset, raw_byte_preview(bytes)),
    );
    let [_, _, hunk, _, _] = regions(area);
    let lines = crate::render::chat::wrapped_lines(
        vec![Line::from(safe(&content))],
        usize::from(hunk.width.saturating_sub(2)),
    );
    super::content_scroll_limit(lines.len(), hunk)
}

fn raw_byte_preview(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len());
    for byte in bytes {
        match *byte {
            b'\n' => output.push('\n'),
            b'\t' => output.push_str("\\t"),
            b'\\' => output.push_str("\\\\"),
            0x20..=0x7e => output.push(char::from(*byte)),
            _ => write!(output, "\\x{byte:02X}").expect("writing into a string cannot fail"),
        }
    }
    output
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
        (
            review.diff_page.as_ref().map_or_else(
                || selected_hunk_lines(review.selected_file(), review.hunk),
                |page| paged_hunk_lines(page, review.raw_line.as_ref()),
            ),
            hunk,
        )
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
                .title(" Review · Tab focus · ↑↓ select · PgUp/PgDn scroll · n/p diff page · [/ ] raw bytes · t raw · r refresh "),
        ),
        area,
    );
}

fn render_files(
    frame: &mut Frame<'_>,
    area: Rect,
    page: &WorkbenchReviewPage,
    diff_page: Option<&peritus_app_protocol::WorkbenchReviewDiffPage>,
    selected: usize,
    focus: ReviewFocus,
) {
    let items = diff_page.map_or_else(
        || {
            if page.files().is_empty() {
                vec![ListItem::new(Line::styled(
                    "No parsed file changes",
                    Style::default().fg(MUTED),
                ))]
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
            }
        },
        |diff_page| {
            vec![ListItem::new(format!(
                "{}  file {}/{} · hunk {} · {} total hunks",
                safe(diff_page.file_anchor().path()),
                diff_page.query().file_offset().saturating_add(1),
                diff_page.total_files(),
                diff_page.query().hunk_offset().saturating_add(1),
                diff_page.total_hunks(),
            ))]
        },
    );
    let mut state = ListState::default();
    if diff_page.is_some() || !page.files().is_empty() {
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

fn paged_hunk_lines(
    page: &peritus_app_protocol::WorkbenchReviewDiffPage,
    raw_line: Option<&(u32, Vec<u8>)>,
) -> Vec<Line<'static>> {
    let mut lines = page.hunk().map_or_else(Vec::new, |hunk| {
        vec![Line::styled(safe(hunk.header()), Style::default().fg(ACCENT))]
    });
    lines.extend(page.lines().iter().map(|line| {
        let (prefix, color) = match line.kind() {
            WorkbenchDiffLineKind::Context => (" ", Color::White),
            WorkbenchDiffLineKind::Removed => ("-", BAD),
            WorkbenchDiffLineKind::Added => ("+", GOOD),
            WorkbenchDiffLineKind::Metadata => ("\\", MUTED),
        };
        let content = raw_line
            .filter(|(offset, _)| {
                *offset >= line.raw_offset()
                    && offset.saturating_sub(line.raw_offset()) < line.raw_length()
            })
            .map_or_else(
                || line.preview().to_owned(),
                |(offset, bytes)| format!("[raw {offset}..] {}", raw_byte_preview(bytes)),
            );
        Line::styled(format!("{prefix}{}", safe(&content)), Style::default().fg(color))
    }));
    lines
}

#[allow(
    clippy::too_many_arguments,
    reason = "the bounded diff renderer receives one snapshot of its focused layout state"
)]
fn render_hunk(
    frame: &mut Frame<'_>,
    area: Rect,
    page: Option<&peritus_app_protocol::WorkbenchReviewDiffPage>,
    raw_line: Option<&(u32, Vec<u8>)>,
    file: Option<&WorkbenchDiffFile>,
    selected_hunk: usize,
    focus: ReviewFocus,
    scroll: u16,
) {
    let source_lines = page.map_or_else(
        || selected_hunk_lines(file, selected_hunk),
        |page| paged_hunk_lines(page, raw_line),
    );
    let lines =
        crate::render::chat::wrapped_lines(source_lines, usize::from(area.width.saturating_sub(2)));
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

#[cfg(test)]
mod tests {
    use super::{paged_hunk_lines, raw_byte_preview};

    #[test]
    fn raw_byte_preview_escapes_utf8_fragments_without_loss() {
        assert_eq!(raw_byte_preview(&[0xc3]), r"\xC3");
        assert_eq!(raw_byte_preview(&[0xa9, b'\n']), "\\xA9\n");
        assert_eq!(raw_byte_preview(br"\xC3"), r"\\xC3");
        assert_ne!(raw_byte_preview(&[0xc3]), raw_byte_preview(br"\xC3"));
        assert_eq!(raw_byte_preview(b"\t"), r"\t");
    }

    #[test]
    fn truncated_line_raw_page_uses_exact_byte_escapes_too() {
        let run = peritus_types::RunId::new([3; 16]).unwrap();
        let workspace = peritus_types::WorkspaceId::new([4; 16]).unwrap();
        let query = peritus_app_protocol::WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([5; 16]).unwrap(),
            workspace,
        );
        let candidate = peritus_types::Sha256Digest::new([6; 32]);
        let raw = format!(
            "diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -0,0 +1 @@\n+{}é{}\n",
            "a".repeat(32 * 1024 - 1),
            "b".repeat(5_000),
        );
        let diff_page = peritus_app_protocol::parse_workbench_diff_page(
            peritus_app_protocol::WorkbenchReviewDiffQuery::new(query, run, 7, 0, 0, 0),
            candidate,
            &raw,
        )
        .expect("bounded line page");
        let line = diff_page.lines().first().expect("changed line");
        assert!(line.is_truncated());
        let split_fragment = (line.raw_offset() + 32 * 1024 - 1, vec![0xc3]);
        let rendered = paged_hunk_lines(&diff_page, Some(&split_fragment));
        let text = rendered[1].spans.iter().map(|span| span.content.as_ref()).collect::<String>();
        assert!(text.contains("\\xC3"));
    }
}
