//! Key-reference content and bounded scrolling shared by input and rendering.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, text::Line};

const LINES: &[&str] = &[
    "Conversation modes",
    "  /modules · /module ID TASK · /module show · /module resume",
    "  /design · /design show · /design build",
    crate::modes::description(peritus_app_protocol::ProductInteractionMode::Design),
    "  /chat",
    crate::modes::description(peritus_app_protocol::ProductInteractionMode::Chat),
    "  /plan",
    crate::modes::description(peritus_app_protocol::ProductInteractionMode::Plan),
    "  /review",
    crate::modes::description(peritus_app_protocol::ProductInteractionMode::Review),
    "  /build",
    crate::modes::description(peritus_app_protocol::ProductInteractionMode::Build),
    "  Read-only tools: list, read, search, selected attachments and image/screen observations.",
    "  Mode changes are explicit. Stop active work before changing mode.",
    "  Switching mode does not grant workspace permissions; /permissions inspects them.",
    "  In an ordinary folder, use /chat for changes; /build requires a managed Git workspace.",
    "",
    "Message editing",
    "  Right-click the composer for Copy/Paste; Linux middle-click pastes the primary selection.",
    "  Mouse paste edits the draft only. F2 explicitly enters native terminal output selection.",
    "  Home/End, Ctrl-A/E, Cmd/Super-Left/Right: current line start/end; Ctrl-Home/End: message start/end.",
    "  Left/Right or Ctrl-B: move by character; Ctrl-P/N: move by row.",
    "  Up/Down: move by row; at the first/last row, recall older/newer messages. Esc restores the draft.",
    "  F3: search saved conversation messages. Enter loads an editable draft; Esc returns.",
    "  Convenience defaults (use /keys for your configured bindings):",
    "  Ctrl-F: find public transcript text; Ctrl-O or @: choose a project file.",
    "  F4: inspect steering; arrows select, e edit, h hold/release, Delete withdraw.",
    "  Queue editor: Enter saves, Shift-Enter newline, Esc cancels; main draft stays.",
    "  Ctrl-G: edit the unsent draft in VISUAL/EDITOR; Ctrl-V: clipboard image preview.",
    "  Alt-C: copy last answer; Alt-Y: choose a code block to copy.",
    "  F9 or /keys: remap convenience shortcuts; /keys reset restores defaults.",
    "  F10 or /theme: preview and save dark, light, or plain syntax colors.",
    "  Ctrl/Option/Alt-Left/Right or Alt-b/f: move by word. Shift + motion extends selection.",
    "  Backspace/Ctrl-H and Delete/Ctrl-D: delete character left/right.",
    "  Ctrl-W, Ctrl/Option/Alt-Backspace, Ctrl-Alt-H: delete word left.",
    "  Alt-d, Ctrl/Option/Alt-Delete: delete word right; Ctrl-U/Super-Backspace: kill to line start.",
    "  Ctrl-K: kill to line end (or join next line at its end). Ctrl-Y: paste the last Ctrl-K/U/W deletion.",
    "  Cmd keys require a compatible terminal; terminal/OS shortcuts may intercept them.",
    "",
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
    "  y              copy the selected run identity (Runs)",
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
            "Conversation modes"
                | "Message editing"
                | "Navigation"
                | "Live connection"
                | "Terminal"
                | "Authority boundary"
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
