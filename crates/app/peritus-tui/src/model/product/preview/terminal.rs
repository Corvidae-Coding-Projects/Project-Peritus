//! Explicit attachment to the latest running interactive preview.

use super::{AppModel, Effect, NoticeLevel};

impl AppModel {
    pub(super) fn attach_preview_terminal(&mut self) -> Vec<Effect> {
        let Some((query, _)) = self.preview_binding() else { return Vec::new() };
        let process = self
            .product
            .as_ref()
            .and_then(|product| product.preview.as_ref())
            .filter(|page| page.query() == query)
            .and_then(|page| {
                page.launches().iter().rev().find(|launch| {
                    launch.profile().interactive()
                        && launch.state() == peritus_app_protocol::WorkbenchLaunchState::Running
                })
            })
            .and_then(peritus_app_protocol::WorkbenchLaunchResult::process);
        let Some(process) = process else {
            self.notice(
                NoticeLevel::Warning,
                "Launch an interactive preview before opening its terminal.",
            );
            return Vec::new();
        };
        if let Some(terminal) = &mut self.terminal
            && terminal.can_capture()
        {
            if terminal.binding().process_id() == process {
                terminal.set_capture_input(true);
                self.view = crate::model::View::Terminal;
                self.clear_chat_command();
            } else {
                self.notice(NoticeLevel::Info, "Another terminal is attached. Release capture with Ctrl-], then press d to detach before switching.");
            }
            return Vec::new();
        }
        self.attach_terminal(&crate::model::format_id(process.as_bytes()))
    }
}
