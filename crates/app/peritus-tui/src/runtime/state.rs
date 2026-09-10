//! Process-local presentation retention across a launcher-owned daemon recovery.

use super::TuiConfig;
use crate::{action::Action, model::AppModel};

/// Bounded, process-local UI state retained while the launcher restores daemon readiness.
///
/// Reuse this value only with the same configuration. A different endpoint or workspace starts
/// a fresh interface; no draft or selected model is transferred between workspaces.
#[derive(Debug, Default)]
pub struct TuiState {
    saved: Option<(TuiConfig, Box<AppModel>)>,
}

impl TuiState {
    pub(super) fn take_model(&mut self, config: &TuiConfig, seed: [u8; 32]) -> Box<AppModel> {
        self.saved.take().filter(|(previous, _)| previous == config).map_or_else(
            || Box::new(AppModel::with_product(seed, config.product().cloned())),
            |(_, model)| model,
        )
    }

    pub(super) fn retain(&mut self, config: TuiConfig, mut model: Box<AppModel>) {
        // Recover unacknowledged text without retaining transport-bound requests or authority.
        let _ = model.update(Action::Disconnected("restoring daemon readiness".to_owned()));
        model.terminal = None;
        model.prompts.clear();
        model.editor = None;
        if let Some(product) = &mut model.product {
            product.confirmation = None;
        }
        self.saved = Some((config, model));
    }
}
