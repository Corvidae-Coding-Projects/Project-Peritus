//! Preview launch, capture, interaction, and human-feedback evidence rendering.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::detail::safe;
use crate::{
    model::{AppModel, format_digest, format_id},
    render::{BAD, GOOD, MUTED, WARN, field},
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let Some(product) = &model.product else { return };
    let Some(page) = product.preview.as_ref().filter(|page| {
        product.selected_run().is_some_and(|run| run.run_id() == page.query().run())
    }) else {
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::styled("No preview result page loaded.", Style::default().fg(MUTED)),
                Line::from(
                    "Use /preview results from the selected Workbench conversation and run.",
                ),
                Line::from("Launching and inspecting are separate explicit operations."),
            ]))
            .block(Block::default().borders(Borders::ALL).title(" Preview results "))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    };
    let capability = match page.capability() {
        peritus_app_protocol::WorkbenchCaptureCapability::X11SelectedWindow => {
            "X11 selected-window capture available".to_owned()
        }
        peritus_app_protocol::WorkbenchCaptureCapability::Unavailable(detail) => {
            format!("capture unavailable: {}", safe(detail.as_str()))
        }
    };
    let mut lines = vec![
        field("Run", format_id(page.query().run().as_bytes())),
        field("Conversation", format_id(page.query().query().conversation().as_bytes())),
        field("Workspace", format_id(page.query().query().workspace().as_bytes())),
        field("Control revision", page.control_revision().to_string()),
        field("Result revision", page.result_revision().to_string()),
        field("Capture capability", capability),
        Line::from(""),
    ];
    if page.launches().is_empty() {
        lines.push(Line::styled("No preview launches admitted.", Style::default().fg(MUTED)));
    } else {
        for (index, launch) in page.launches().iter().enumerate() {
            lines.extend(preview_launch_lines(index, launch));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Preview evidence · PgUp/PgDn scroll · r refresh · Esc conversation "),
            )
            .scroll((product.preview_scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn preview_launch_lines(
    index: usize,
    launch: &peritus_app_protocol::WorkbenchLaunchResult,
) -> Vec<Line<'static>> {
    let mut lines = launch_summary_lines(index, launch);
    append_interaction_lines(&mut lines, launch);
    append_capture_lines(&mut lines, launch);
    append_feedback_lines(&mut lines, launch);
    lines.push(Line::from(""));
    lines
}

fn launch_summary_lines(
    index: usize,
    launch: &peritus_app_protocol::WorkbenchLaunchResult,
) -> Vec<Line<'static>> {
    let profile = launch.profile();
    let evidence = launch.evidence();
    vec![
        Line::styled(
            format!("Launch {} · {:?}", index + 1, launch.state()),
            phase_style_for_preview(launch.state()).add_modifier(Modifier::BOLD),
        ),
        field("Operation", format_id(launch.launch().as_bytes())),
        field(
            "Process",
            launch.process().map_or_else(
                || "none observed".to_owned(),
                |process| format_id(process.as_bytes()),
            ),
        ),
        field(
            "Command",
            format!(
                "{} {}",
                safe(profile.executable().as_str()),
                profile
                    .arguments()
                    .iter()
                    .map(|argument| safe(argument.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .trim_end()
            .to_owned(),
        ),
        field("Working directory", safe(profile.working_directory().as_str())),
        field(
            "Source",
            format!(
                "{:?} · {} · {}",
                profile.source().kind(),
                safe(profile.source().path().as_str()),
                format_digest(profile.source().digest().as_bytes())
            ),
        ),
        field(
            "Build",
            profile.build().map_or_else(
                || "not supplied".to_owned(),
                |build| {
                    format!(
                        "{} · {}",
                        safe(build.path().as_str()),
                        format_digest(build.digest().as_bytes())
                    )
                },
            ),
        ),
        field("Ready", launch.ready().to_string()),
        field(
            "Evidence",
            format!(
                "built={} launched={} captured={} behavior-checked={} human-reviewed={}",
                evidence.built(),
                evidence.launched(),
                evidence.captured(),
                evidence.behavior_checked(),
                evidence.human_reviewed()
            ),
        ),
        field("Behavior checks", launch.behavior_checks().to_string()),
        field(
            "Stdout digest",
            launch.stdout_digest().map_or_else(
                || "not settled".to_owned(),
                |digest| format_digest(digest.as_bytes()),
            ),
        ),
        field(
            "Exit code",
            launch.exit_code().map_or_else(|| "not observed".to_owned(), |code| code.to_string()),
        ),
    ]
}

fn append_interaction_lines(
    lines: &mut Vec<Line<'static>>,
    launch: &peritus_app_protocol::WorkbenchLaunchResult,
) {
    for interaction in launch.interactions() {
        lines.push(field(
            "Input receipt",
            format!(
                "{} · {} · accepted={}",
                format_id(interaction.operation().as_bytes()),
                format_digest(interaction.digest().as_bytes()),
                interaction.observed()
            ),
        ));
    }
}

fn append_capture_lines(
    lines: &mut Vec<Line<'static>>,
    launch: &peritus_app_protocol::WorkbenchLaunchResult,
) {
    for capture in launch.captures() {
        let target = match capture.target() {
            peritus_app_protocol::WorkbenchCaptureTarget::X11Window(window) => {
                format!("X11 window 0x{window:x}")
            }
        };
        lines.push(field(
            "Capture",
            format!(
                "{} · {:?} · {} · {}",
                format_id(capture.operation().as_bytes()),
                capture.state(),
                target,
                safe(capture.detail().as_str())
            ),
        ));
        if let Some(artifact) = capture.artifact() {
            let dimensions = capture.dimensions().map_or_else(
                || "unknown size".to_owned(),
                |(width, height)| format!("{width}x{height}"),
            );
            lines.push(field(
                "Capture artifact",
                format!(
                    "{} · {} · {}",
                    format_id(artifact.as_bytes()),
                    capture.image_digest().map_or_else(
                        || "digest unavailable".to_owned(),
                        |digest| format_digest(digest.as_bytes())
                    ),
                    dimensions
                ),
            ));
        }
    }
}

fn append_feedback_lines(
    lines: &mut Vec<Line<'static>>,
    launch: &peritus_app_protocol::WorkbenchLaunchResult,
) {
    for feedback in launch.feedback() {
        let region = feedback.region().map_or_else(
            || "whole capture".to_owned(),
            |region| {
                let (x, y, width, height) = region.coordinates();
                format!("x={x} y={y} {width}x{height}")
            },
        );
        lines.push(field(
            "Human feedback",
            format!(
                "{} · {:?} · capture {} · {} · {}",
                format_id(feedback.operation().as_bytes()),
                feedback.feedback(),
                format_id(feedback.capture().as_bytes()),
                region,
                safe(feedback.message().as_str())
            ),
        ));
    }
}

fn phase_style_for_preview(state: peritus_app_protocol::WorkbenchLaunchState) -> Style {
    match state {
        peritus_app_protocol::WorkbenchLaunchState::Running
        | peritus_app_protocol::WorkbenchLaunchState::Exited => Style::default().fg(GOOD),
        peritus_app_protocol::WorkbenchLaunchState::Accepted => Style::default().fg(WARN),
        peritus_app_protocol::WorkbenchLaunchState::Stopped => Style::default().fg(MUTED),
        peritus_app_protocol::WorkbenchLaunchState::Failed => Style::default().fg(BAD),
    }
}
