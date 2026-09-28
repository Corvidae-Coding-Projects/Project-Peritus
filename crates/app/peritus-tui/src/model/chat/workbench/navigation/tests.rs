//! Human scrolling remains bounded across every metadata inspector at small terminal sizes.

use super::*;
use crate::{action::Action, model::View};
use crossterm::event::{Event, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

fn key(model: &mut AppModel, code: KeyCode) {
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))));
}

#[test]
fn goal_context_and_attachment_panels_bound_navigation_after_resize() {
    for kind in 0..6 {
        let mut model = AppModel::new([88; 32]);
        model.view = View::Conversation;
        model.chat.workbench.open = true;
        model.chat.workbench.message = "long inspection text 界 ".repeat(200);
        match kind {
            0 => model.chat.workbench.goal_mode = true,
            1 => {
                model.chat.workbench.context_mode =
                    Some(peritus_app_protocol::WorkbenchContextView::Next);
            }
            2 | 3 => {
                model.chat.workbench.files.open = true;
                model.chat.workbench.files.list = kind == 3;
            }
            _ => {
                model.chat.workbench.images.open = true;
                model.chat.workbench.images.list = kind == 5;
            }
        }
        model.chat.viewport = Some(Rect::new(0, 0, 40, 24));
        key(&mut model, KeyCode::End);
        let narrow = model.chat.workbench.scroll;
        assert!(narrow > 0);
        model.chat.viewport = Some(Rect::new(0, 0, 120, 24));
        key(&mut model, KeyCode::End);
        let wide = model.chat.workbench.scroll;
        assert!(wide > 0 && wide < narrow, "resize did not update panel {kind}");
        let bottom = screen(&model, 120);
        for _ in 0..10 {
            key(&mut model, KeyCode::PageDown);
        }
        assert_eq!(model.chat.workbench.scroll, wide);
        assert_eq!(screen(&model, 120), bottom);
        key(&mut model, KeyCode::Up);
        assert_eq!(model.chat.workbench.scroll, wide - 1);
        key(&mut model, KeyCode::Home);
        assert_eq!(model.chat.workbench.scroll, 0);
    }
}

fn screen(model: &AppModel, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
    terminal.draw(|frame| crate::render::draw(frame, model)).unwrap();
    terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect()
}

#[test]
fn every_metadata_inspector_reaches_its_end_and_reverses_without_blank_overscroll() {
    for width in [48, 80, 120] {
        for mode in [
            WorkbenchMode::Sessions,
            WorkbenchMode::Queue,
            WorkbenchMode::Brief,
            WorkbenchMode::Compaction,
            WorkbenchMode::Checkpoints,
            WorkbenchMode::Permissions,
            WorkbenchMode::Init,
            WorkbenchMode::Memory,
        ] {
            let mut model = AppModel::new([87; 32]);
            model.view = View::Conversation;
            model.chat.viewport = Some(Rect::new(0, 0, width, 24));
            model.chat.workbench.open = true;
            model.chat.workbench.mode = mode;
            model.chat.workbench.message =
                format!("WORKBENCH-TOP {}\nWORKBENCH-END", "wide 界 row ".repeat(200));
            let top = screen(&model, width);
            assert!(top.contains("WORKBENCH-TOP"));
            key(&mut model, KeyCode::End);
            let bottom = model.chat.workbench.scroll;
            assert!(bottom > 0, "End did not scroll {mode:?} at {width} columns");
            let end = screen(&model, width);
            assert_ne!(top, end);
            assert!(
                end.contains("WORKBENCH-END"),
                "last message line unavailable in {mode:?} at {width} columns, offset {bottom}: {end}"
            );
            for _ in 0..100 {
                key(&mut model, KeyCode::PageDown);
            }
            assert_eq!(model.chat.workbench.scroll, bottom);
            assert_eq!(screen(&model, width), end);
            key(&mut model, KeyCode::Up);
            assert_eq!(model.chat.workbench.scroll, bottom - 1);
            key(&mut model, KeyCode::Home);
            assert_eq!(model.chat.workbench.scroll, 0);
            assert_eq!(screen(&model, width), top);
        }
    }
}
