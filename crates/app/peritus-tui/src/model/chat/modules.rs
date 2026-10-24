//! Keyboard market selection and explicit install/enable/use actions, independent of inference.
use crate::model::{AppModel, Effect};
use crossterm::event::{KeyCode, KeyEvent};
use peritus_modules::ModuleOperation;
use serde_json::Value;
use std::fmt::Write as _;

#[derive(Debug, Default)]
pub struct ModuleMenu {
    pub(crate) rows: Vec<Value>,
    pub(crate) selected: usize,
    pub(crate) busy: bool,
    pub(crate) status: String,
    pub(crate) session: Value,
}
impl AppModel {
    pub(super) fn module_menu_command(&mut self) -> Vec<Effect> {
        self.clear_chat_command();
        self.chat.module_menu = Some(ModuleMenu::default());
        let operation = self.chat.run_id.map_or(ModuleOperation::Browse, |id| {
            ModuleOperation::Session { run: encode_hex(id.as_bytes()) }
        });
        self.module_operation(operation)
    }
    fn module_operation(&mut self, operation: ModuleOperation) -> Vec<Effect> {
        let Some(menu) = self.chat.module_menu.as_mut() else { return Vec::new() };
        if menu.busy {
            return Vec::new();
        }
        menu.busy = true;
        menu.status = "Working…".into();
        self.chat.module_generation = self.chat.module_generation.wrapping_add(1).max(1);
        vec![Effect::ModuleOperation { generation: self.chat.module_generation, operation }]
    }
    pub(in crate::model) fn module_result(
        &mut self,
        generation: u64,
        result: Result<Value, String>,
    ) -> Vec<Effect> {
        if generation != 0 && generation != self.chat.module_generation {
            return Vec::new();
        }
        let Some(menu) = self.chat.module_menu.as_mut() else { return Vec::new() };
        menu.busy = false;
        let value = match result {
            Ok(value) => value,
            Err(e) => {
                menu.status = e;
                return Vec::new();
            }
        };
        if !value["session"].is_null() {
            menu.session = value["session"].clone();
        }
        let mut rows = std::collections::BTreeMap::new();
        for row in value["installed"].as_array().into_iter().flatten() {
            let mut row = row.clone();
            row["installed"] = Value::Bool(true);
            rows.insert(row["id"].as_str().unwrap_or_default().to_owned(), row);
        }
        for entry in value["modules"].as_array().into_iter().flatten() {
            let id = entry["id"].as_str().unwrap_or_default().to_owned();
            if let Some(row) = rows.get_mut(&id) {
                row["market_digest"] = entry["digest"].clone();
                row["market_version"] = entry["version"].clone();
            } else {
                rows.insert(id, entry.clone());
            }
        }
        for entry in value["sources"].as_array().into_iter().flatten() {
            let id = entry["id"].as_str().unwrap_or_default().to_owned();
            if let Some(row) = rows.get_mut(&id) {
                row["source"] = Value::Bool(true);
                row["source_digest"] = entry["source_digest"].clone();
                row["source_version"] = entry["version"].clone();
                row["source_target"] = entry["target"].clone();
            } else {
                let mut row = entry.clone();
                row["source"] = Value::Bool(true);
                rows.insert(id, row);
            }
        }
        for build in value["source_builds"].as_array().into_iter().flatten() {
            if let Some(row) = build["module_id"].as_str().and_then(|id| rows.get_mut(id)) {
                row["source_build"] = build.clone();
            }
        }
        menu.rows = rows.into_values().collect();
        menu.selected = menu.selected.min(menu.rows.len().saturating_sub(1));
        menu.status = value["notice"]
            .as_str()
            .unwrap_or("Packages install disabled. Enable explicitly, then Enter to select.")
            .into();
        if value["commit"].is_null()
            && value["notice"].as_str().is_some_and(|n| n.starts_with("Refresh the market"))
        {
            return self.module_operation(ModuleOperation::Refresh);
        }
        Vec::new()
    }
    pub(in crate::model) fn module_menu_tick(&mut self) -> Vec<Effect> {
        let Some(menu) = self.chat.module_menu.as_ref() else { return Vec::new() };
        if menu.busy
            || !menu.rows.iter().any(|row| {
                row["source_build"]["phase"].as_str().is_some_and(|phase| {
                    matches!(
                        phase,
                        "queued"
                            | "preparing_source"
                            | "preparing_dependencies"
                            | "compiling"
                            | "packaging"
                    )
                })
            })
        {
            return Vec::new();
        }
        self.module_operation(ModuleOperation::Browse)
    }
    pub(super) fn module_menu_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(menu) = self.chat.module_menu.as_mut() else { return Vec::new() };
        if key.code == KeyCode::Esc {
            self.chat.module_menu = None;
            return Vec::new();
        }
        if menu.busy {
            return Vec::new();
        }
        match key.code {
            KeyCode::Up => menu.selected = menu.selected.saturating_sub(1),
            KeyCode::Down => {
                menu.selected = (menu.selected + 1).min(menu.rows.len().saturating_sub(1));
            }
            KeyCode::Home => menu.selected = 0,
            KeyCode::End => menu.selected = menu.rows.len().saturating_sub(1),
            KeyCode::Char('r') => return self.module_operation(ModuleOperation::Refresh),
            _ => {}
        }
        let Some(row) = menu.rows.get(menu.selected) else { return Vec::new() };
        let id = row["id"].as_str().unwrap_or_default().to_owned();
        if row["enabled"] == true
            && row["error"].is_null()
            && menu.session["module"] == id
            && menu.session["digest"] == row["digest"]
            && let KeyCode::Char(number @ '1'..='9') = key.code
            && let Some(action) = menu.session["view"]["actions"]
                .as_array()
                .and_then(|actions| actions.get(number as usize - '1' as usize))
                .cloned()
        {
            if self.chat_work_active() {
                return Vec::new();
            }
            let command =
                format!("/module {id} {}", action["arguments"].as_str().unwrap_or_default());
            self.chat.module_menu = None;
            if action["submit"] == true {
                return self.slash_command(&command);
            }
            self.chat.buffer = command;
            self.chat.cursor = self.chat.buffer.len();
            self.chat.mode = peritus_app_protocol::ProductInteractionMode::Chat;
            return Vec::new();
        }
        match key.code {
            KeyCode::Char('b') if row["source"] == true => {
                let source_digest = row["source_digest"].as_str().unwrap_or_default().to_owned();
                return self.module_operation(ModuleOperation::BuildInstall { id, source_digest });
            }
            KeyCode::Char('x')
                if row["source_build"]["phase"].as_str().is_some_and(|phase| {
                    matches!(
                        phase,
                        "queued"
                            | "preparing_source"
                            | "preparing_dependencies"
                            | "compiling"
                            | "packaging"
                    )
                }) =>
            {
                return self.module_operation(ModuleOperation::CancelBuild { id });
            }
            KeyCode::Char('i')
                if row["source"] != true && !row["bundled"].as_bool().unwrap_or(false) =>
            {
                let digest = row["market_digest"]
                    .as_str()
                    .or_else(|| row["digest"].as_str())
                    .unwrap_or_default()
                    .into();
                return self.module_operation(ModuleOperation::Install { id, digest });
            }
            KeyCode::Char('e') if row["installed"] == true || row["bundled"] == true => {
                let enabled = !row["enabled"].as_bool().unwrap_or(false);
                return self.module_operation(ModuleOperation::Enable { id, enabled });
            }
            KeyCode::Char('d') if row["installed"] == true && row["bundled"] != true => {
                return self.module_operation(ModuleOperation::Remove { id });
            }
            KeyCode::Enter if row["enabled"] == true && row["error"].is_null() => {
                if self.chat_work_active() {
                    if let Some(menu) = self.chat.module_menu.as_mut() {
                        menu.status="Stop active work before selecting another workflow. Its work remains retained.".into();
                    }
                    return Vec::new();
                }
                self.chat.module_menu = None;
                self.chat.buffer = format!("/module {id} ");
                self.chat.cursor = self.chat.buffer.len();
                self.chat.mode = peritus_app_protocol::ProductInteractionMode::Chat;
            }
            _ => {}
        }
        Vec::new()
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}
