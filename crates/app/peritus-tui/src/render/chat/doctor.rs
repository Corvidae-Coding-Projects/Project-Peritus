//! Responsive read-only diagnostic panel with independent scrolling and visible return action.

use crate::model::AppModel;
use peritus_app_protocol::DoctorStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let Some(panel) = &model.chat.doctor else {
        return;
    };
    let regions =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .split(area);
    frame.render_widget(
        Paragraph::new("Doctor · local observations only")
            .style(Style::default().fg(super::ACCENT)),
        regions[0],
    );
    let mut lines = Vec::new();
    if let Some(error) = &panel.error {
        lines.push(Line::styled(error.clone(), Style::default().fg(super::BAD)));
    } else if let Some(report) = &panel.report {
        let launcher = model.product.as_ref().and_then(|product| product.launch.launcher_report());
        for finding in report
            .findings()
            .iter()
            .chain(launcher.into_iter().flat_map(peritus_app_protocol::DoctorReport::findings))
        {
            let color = match finding.status() {
                DoctorStatus::Healthy => super::GOOD,
                DoctorStatus::Warning => super::WARN,
                DoctorStatus::Blocked => super::BAD,
                DoctorStatus::Unsupported => super::MUTED,
            };
            lines.push(Line::styled(
                format!("{} · {}", finding.check(), finding.status().label()),
                Style::default().fg(color),
            ));
            lines.push(Line::from(finding.observation().to_owned()));
            if !finding.action().is_empty() {
                lines.push(Line::from(format!("Next: {}", finding.action())));
            }
            lines.push(Line::from(""));
        }
        lines.push(Line::from(format!(
            "Negotiated features: {}",
            model
                .features
                .iter()
                .map(peritus_app_protocol::ProtocolFeatureName::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )));
        if launcher.is_none() {
            lines.push(Line::from(
                "Launcher checks · unsupported: this client has no launch-time report.",
            ));
        }
    } else if !matches!(model.connection, crate::model::ConnectionStatus::Online { .. }) {
        lines
            .push(Line::from("Offline. Close this panel and /reconnect; no repair was attempted."));
    } else {
        lines.push(Line::from("Waiting for the daemon's local diagnostic report…"));
    }
    let lines = super::wrapped_lines(lines, usize::from(regions[1].width));
    let offset = panel.scroll.min(lines.len().saturating_sub(usize::from(regions[1].height)));
    frame.render_widget(
        Paragraph::new(
            lines.into_iter().skip(offset).take(usize::from(regions[1].height)).collect::<Vec<_>>(),
        ),
        regions[1],
    );
    frame.render_widget(
        Paragraph::new("Esc back · ↑↓ scroll · r refresh").style(Style::default().fg(super::MUTED)),
        regions[2],
    );
}
