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
        if self.view == View::Diff
            && let Some(effects) = self.review_key(key)
        {
            return Some(effects);
        }
        if matches!(self.view, View::Runs | View::Diff | View::Review | View::Preview)
            && matches!(
                key.code,
                KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End
            )
        {
            let maximum = crate::render::inspection_scroll_limit(self);
            let page = crate::render::inspection_scroll_page(self);
            if let Some(product) = &mut self.product {
                let offset = if self.view == View::Runs {
                    &mut product.detail_scroll
                } else if self.view == View::Preview {
                    &mut product.preview_scroll
                } else if self.view == View::Diff
                    && product.review.page.is_some()
                    && !product.review.raw
                {
                    &mut product.review.scroll
                } else {
                    &mut product.inspection_scroll
                };
                let current = (*offset).min(maximum);
                *offset = match key.code {
                    KeyCode::PageUp => current.saturating_sub(page),
                    KeyCode::PageDown => current.saturating_add(page).min(maximum),
                    KeyCode::End => maximum,
                    _ => 0,
                };
            }
            return Some(Vec::new());
        }
        if self.view == View::Preview && key.code == KeyCode::Char('r') && self.product.is_some() {
            return Some(self.refresh_selected_preview(false));
        }
        if self.view != View::Runs {
            return None;
        }
        match key.code {
            KeyCode::Enter => return Some(self.open_selected_conversation()),
            KeyCode::Char('m') => self.open_product_message_composer(),
            KeyCode::Char('i') => {
                return Some(self.open_diff_panel());
            }
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
            KeyCode::Char('u') => {
                return Some(
                    self.control_selected_product_run(ProductRunControlAction::Acknowledge),
                );
            }
            KeyCode::Char('w') => self.cycle_product_provider(ProviderRole::Writer),
            KeyCode::Char('e') => self.cycle_product_provider(ProviderRole::Reviewer),
            KeyCode::Char('f') => self.cycle_product_provider(ProviderRole::Fixer),
            _ => return None,
        }
        Some(Vec::new())
    }
}
