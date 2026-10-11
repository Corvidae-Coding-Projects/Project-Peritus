//! Key-reference content and bounded scrolling shared by input and rendering.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, text::Line};

const LINES: &[&str] = &[
    "Navigation",
    "  1–8        open Runs, Diff, Review, Trace, Evolution, Terminal, Approvals, Preview",
    "  Tab/Shift-Tab  next/previous view",
    "  j/k or ↑/↓    select an event or prompt",
    "  ?              this help",
    "  n              compose a new coding task (Runs)",
    "  Enter/m        message, redirect, or continue the selected run (Runs)",
    "  w/e/f          cycle writer/reviewer/fixer provider (Runs)",
    "  i/v/a/c/p/D    inspect / run / accept / commit / export / discard (Runs)",
    "  x/r            cancel / retry selected coding run (Runs)",
    "  PageUp/Down    scroll Diff, Review, Preview, or selected approval details",
    "  Home/End       beginning/end of those details",
    "",
    "Live connection",
    "  Ctrl-R         reconnect with drafts retained (unless terminal input is captured)",
    "  R              reconnect from a panel",
    "  p/u            pause/resume event delivery",
    "  Ctrl-Q/Ctrl-C  exit and send bounded detach/cancel cleanup",
    "",
    "Terminal",
    "  a              attach by exact ProcessId",
    "  i              capture keyboard for attached terminal",
    "  Ctrl-]         release terminal keyboard capture",
    "  PageUp/Down    scroll transcript while capture is released",
    "  d/x            detach / request process cancellation",
    "",
    "Authority boundary",
    "  Approval decisions must arrive as externally signed canonical B1 frames.",
    "  A TUI acknowledgement means protocol input only; daemon/domain state remains authoritative.",
    "  Terminal and domain bytes are rendered as inert data with control sequences removed.",
];

pub fn lines(width: u16) -> Vec<Line<'static>> {
    let mut lines = crate::render::wrapped_lines(
        LINES.iter().map(|line| Line::from(*line)).collect(),
        usize::from(width),
    );
    for line in &mut lines {
        if matches!(
            line.to_string().as_str(),
            "Navigation" | "Live connection" | "Terminal" | "Authority boundary"
        ) {
            line.style =
                ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::BOLD);
        }
    }
    lines
}

pub fn scroll(offset: &mut usize, key: KeyEvent, viewport: Option<Rect>) -> bool {
    let viewport = viewport.unwrap_or(Rect::new(0, 0, 80, 24));
    let rows = usize::from(viewport.height.saturating_sub(6)).max(1);
    let maximum = lines(viewport.width.saturating_sub(2)).len().saturating_sub(rows);
    let current = (*offset).min(maximum);
    *offset = match key.code {
        KeyCode::Up | KeyCode::Char('k') => current.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => current.saturating_add(1).min(maximum),
        KeyCode::PageUp => current.saturating_sub(12),
        KeyCode::PageDown => current.saturating_add(12).min(maximum),
        KeyCode::Home => 0,
        KeyCode::End => maximum,
        _ => return false,
    };
    true
}
