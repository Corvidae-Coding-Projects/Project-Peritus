//! Product dashboard keyboard routing.

use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::ProductRunControlAction;

use super::ProviderRole;
use crate::{
    action::Effect,
    model::{AppModel, View},
};

impl AppModel {
    pub(in crate::model) fn handle_product_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        if self.view != View::Runs {
            return None;
        }
        match key.code {
            KeyCode::Char('n') => self.open_task_composer(),
            KeyCode::Enter | KeyCode::Char('m') => self.open_product_message_composer(),
            KeyCode::Char('i') => self.view = View::Diff,
            KeyCode::Char('v') => return Some(self.run_selected_product_candidate()),
            KeyCode::Char('a') => {
                return Some(self.control_selected_product_run(ProductRunControlAction::Accept));
            }
            KeyCode::Char('c') => {
                return Some(self.control_selected_product_run(ProductRunControlAction::Commit));
            }
            KeyCode::Char('p') => {
                return Some(self.control_selected_product_run(ProductRunControlAction::Export));
            }
            KeyCode::Char('D') => {
                return Some(self.control_selected_product_run(ProductRunControlAction::Discard));
            }
            KeyCode::Char('x') => {
                return Some(self.control_selected_product_run(ProductRunControlAction::Cancel));
            }
            KeyCode::Char('r') if self.product.is_some() => {
                return Some(self.control_selected_product_run(ProductRunControlAction::Retry));
            }
            KeyCode::Char('w') => self.cycle_product_provider(ProviderRole::Writer),
            KeyCode::Char('e') => self.cycle_product_provider(ProviderRole::Reviewer),
            KeyCode::Char('f') => self.cycle_product_provider(ProviderRole::Fixer),
            _ => return None,
        }
        Some(Vec::new())
    }
}
